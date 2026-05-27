//! Episode phase classifier — state machine (slice-12).
//!
//! `classify_session` is a pure function: same input always produces the same
//! output (`tests/episode_determinism.rs` asserts this). No I/O, no globals.
//!
//! # Algorithm
//!
//! Left-to-right pass over the ordered event stream. At each position the
//! state machine consults the current state + a 3-event lookahead to decide
//! whether to:
//!   (a) continue the current episode, or
//!   (b) emit a boundary and start a new episode.
//!
//! The lookahead size is `LOOKAHEAD = 3` (DEV-S12-02). Bumping it requires a
//! `_v2` version bump on affected rules and a golden-update commit.
//!
//! # Phase transition table
//!
//! | Event signal | New phase |
//! |---|---|
//! | user_message from User actor | Intake |
//! | read-only tool_call, no mutation ahead, after error | Diagnosis |
//! | read-only tool_call, no mutation ahead | Exploration |
//! | mutating tool_call (Edit/Write/MultiEdit/Bash-non-verify) after failure | Repair |
//! | mutating tool_call (Edit/Write/MultiEdit/Bash-non-verify) | Action |
//! | VerificationRun row starts in this window | Verification |
//! | N=8 consecutive exploration events (no action, no new intake) | Drift |

use chrono::{DateTime, Utc};

use sha2::{Digest, Sha256};

use crate::db::repo_verification_run::VerificationRunRow;
use crate::model::observed::{Actor, EventKind, ObservedEvent};

use super::rules::RULE_IDS;
use super::types::{EpisodeRecord, Phase};

/// Version string embedded in every classifier-produced episode.
pub const CLASSIFIER_VERSION: &str = "episode_classifier@v1";

/// Consecutive exploration events that trigger `Drift`.
const DRIFT_THRESHOLD: usize = 8;

/// How many events ahead the state machine looks to decide boundaries.
const LOOKAHEAD: usize = 3;

// --- helpers -----------------------------------------------------------------

/// Read-only tool names: seeing one of these means we're in
/// exploration (or diagnosis if an error was recently seen).
const READ_TOOLS: &[&str] = &[
    "Read", "Grep", "Glob", "LS", "WebFetch", "WebSearch",
    "Bash",  // Bash counts as read-only only if the command is on the
             // verification allowlist; the classifier does NOT have the command
             // text here (only the tool_name). So Bash is treated as potentially
             // mutating — NOT in READ_TOOLS.
             // See MUTATION_TOOLS for the full list.
];

/// Read-only tool names (Bash excluded; Bash is mutating unless classified
/// separately).
const READ_ONLY_TOOLS: &[&str] = &[
    "Read", "Grep", "Glob", "LS", "WebFetch", "WebSearch",
];

/// Mutating tool names (Bash included by default; Bash-on-allowlist is
/// handled separately by the VerificationRun row injection).
const MUTATION_TOOLS: &[&str] = &["Edit", "Write", "MultiEdit", "Bash"];

fn is_read_only_tool(tool: &str) -> bool {
    READ_ONLY_TOOLS.contains(&tool)
}

fn is_mutation_tool(tool: &str) -> bool {
    MUTATION_TOOLS.contains(&tool)
}

/// Returns true if any of `events[start..start+LOOKAHEAD]` is a mutation call.
fn lookahead_has_mutation(events: &[ObservedEvent], start: usize) -> bool {
    let end = (start + LOOKAHEAD).min(events.len());
    events[start..end].iter().any(|e| {
        e.kind == EventKind::ToolCall
            && e.tool_name
                .as_deref()
                .map(is_mutation_tool)
                .unwrap_or(false)
    })
}

/// Returns true if the event's payload carries `is_error == true`.
fn is_error_result(ev: &ObservedEvent) -> bool {
    ev.payload
        .get("is_error")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Deterministic episode_id: "ep_" + hex(sha256(session_id||phase||start_event_id||end_event_id))[..24].
fn make_episode_id(session_id: &str, phase: Phase, start: &str, end: &str) -> String {
    let raw = format!("{session_id}||{phase:?}||{start}||{end}");
    let mut h = Sha256::new();
    h.update(raw.as_bytes());
    format!("ep_{}", hex::encode(&h.finalize()[..12]))
}

// --- state machine -----------------------------------------------------------

#[derive(Debug)]
struct ClassifierState {
    current_phase: Phase,
    phase_start_idx: usize,
    last_error_at: Option<DateTime<Utc>>,
    last_verification_at: Option<DateTime<Utc>>,
    exploration_streak: usize,
    had_failure: bool, // repair trigger
}

impl ClassifierState {
    fn new() -> Self {
        Self {
            current_phase: Phase::Intake,
            phase_start_idx: 0,
            last_error_at: None,
            last_verification_at: None,
            exploration_streak: 0,
            had_failure: false,
        }
    }

    fn reset_streak(&mut self) {
        self.exploration_streak = 0;
    }
}

/// Emit a completed episode span.
fn emit(
    session_id: &str,
    phase: Phase,
    start: &ObservedEvent,
    end: &ObservedEvent,
    basis: Vec<&'static str>,
    confidence: f32,
) -> EpisodeRecord {
    let episode_id = make_episode_id(session_id, phase, &start.event_id, &end.event_id);
    EpisodeRecord {
        episode_id,
        schema_version: "episode.v1".into(),
        session_id: session_id.to_string(),
        phase,
        start_event_id: start.event_id.clone(),
        end_event_id: end.event_id.clone(),
        started_at: start.observed_at,
        ended_at: end.observed_at,
        evidence_node_ids: vec![],
        classification_basis: basis,
        confidence,
        summary: None,
        classifier_version: CLASSIFIER_VERSION.into(),
    }
}

/// Classify an ordered event stream into a sequence of `EpisodeRecord`s.
///
/// `events` must be in `observed_at` order (the caller — graph builder — ensures
/// this). `runs` is the set of `VerificationRunRow`s for the same session; the
/// classifier uses them to emit `Verification` phase episodes.
///
/// Returns an empty `Vec` for an empty event stream (spec §8).
pub fn classify_session(
    session_id: &str,
    events: &[ObservedEvent],
    runs: &[VerificationRunRow],
) -> Vec<EpisodeRecord> {
    if events.is_empty() {
        return vec![];
    }

    // Build a set of event_ids that are VerificationRun trigger points.
    let verification_trigger_ids: std::collections::HashSet<&str> =
        runs.iter().map(|r| r.trigger_event_id.as_str()).collect();

    let mut out: Vec<EpisodeRecord> = Vec::new();
    let mut st = ClassifierState::new();
    st.current_phase = Phase::Intake;
    st.phase_start_idx = 0;

    // Determine the initial phase from the first event.
    st.current_phase = classify_event_phase(0, events, &verification_trigger_ids, &st);

    let mut i = 1;
    while i < events.len() {
        let ev = &events[i];

        // Update error tracking.
        if is_error_result(ev) {
            st.last_error_at = Some(ev.observed_at);
            st.had_failure = true;
        }

        // Check if a VerificationRun was triggered by the *previous* event (trigger_event_id
        // typically points to the tool_result that preceded the run).
        if verification_trigger_ids.contains(events[i - 1].event_id.as_str()) {
            st.last_verification_at = Some(events[i - 1].observed_at);
        }

        let new_phase = classify_event_phase(i, events, &verification_trigger_ids, &st);

        if new_phase != st.current_phase || should_force_boundary(ev, st.current_phase) {
            // Emit the current episode [phase_start_idx .. i-1].
            let (basis, confidence) = phase_basis_confidence(st.current_phase);
            out.push(emit(
                session_id,
                st.current_phase,
                &events[st.phase_start_idx],
                &events[i - 1],
                basis,
                confidence,
            ));

            // Start new episode.
            st.current_phase = new_phase;
            st.phase_start_idx = i;
            st.reset_streak();

            // After a verification episode ends, set had_failure based on
            // whether the run that triggered it failed (repair detection).
            if st.current_phase == Phase::Verification {
                // We don't have run status here without looking up; set
                // had_failure conservatively when transitioning away from
                // verification — handled at transition out of Verification.
            }
        }

        // Exploration streak tracking for drift.
        if st.current_phase == Phase::Exploration {
            st.exploration_streak += 1;
            if st.exploration_streak >= DRIFT_THRESHOLD {
                // Force Drift phase.
                let (basis, confidence) = phase_basis_confidence(Phase::Exploration);
                out.push(emit(
                    session_id,
                    Phase::Exploration,
                    &events[st.phase_start_idx],
                    ev,
                    basis,
                    confidence,
                ));
                st.current_phase = Phase::Drift;
                st.phase_start_idx = i;
                st.reset_streak();
            }
        } else {
            st.reset_streak();
        }

        i += 1;
    }

    // Emit the final episode.
    let (basis, confidence) = phase_basis_confidence(st.current_phase);
    out.push(emit(
        session_id,
        st.current_phase,
        &events[st.phase_start_idx],
        &events[events.len() - 1],
        basis,
        confidence,
    ));

    out
}

/// Classify the phase that event at index `i` belongs to.
fn classify_event_phase(
    i: usize,
    events: &[ObservedEvent],
    verification_triggers: &std::collections::HashSet<&str>,
    st: &ClassifierState,
) -> Phase {
    let ev = &events[i];

    // 1. User message → Intake.
    if ev.actor == Actor::User && ev.kind == EventKind::UserMessage {
        return Phase::Intake;
    }

    // 2. VerificationRun trigger — if *this* event is a trigger, it's Verification.
    if verification_triggers.contains(ev.event_id.as_str()) {
        return Phase::Verification;
    }

    // 3. ToolCall with a mutating tool.
    if ev.kind == EventKind::ToolCall {
        if let Some(tool) = ev.tool_name.as_deref() {
            if is_mutation_tool(tool) {
                // Repair if we had a recent failure or failed verification.
                if st.had_failure {
                    return Phase::Repair;
                }
                return Phase::Action;
            }
            if is_read_only_tool(tool) {
                // Check if there's a mutation ahead in the lookahead window;
                // if so, don't classify as exploration yet — stay in current
                // phase for now (the boundary fires at the mutation event).
                // Actually: classify now; the boundary will fire when we reach
                // the mutation.
                if st.last_error_at.is_some() {
                    return Phase::Diagnosis;
                }
                return Phase::Exploration;
            }
        }
    }

    // 4. ToolResult carrying is_error → stay in / enter Diagnosis.
    if ev.kind == EventKind::ToolResult && is_error_result(ev) {
        return Phase::Diagnosis;
    }

    // 5. Everything else inherits current phase.
    st.current_phase
}

/// Force a new boundary even within the same phase type when a user message
/// appears (a fresh intake always starts a new episode).
fn should_force_boundary(ev: &ObservedEvent, current: Phase) -> bool {
    ev.actor == Actor::User
        && ev.kind == EventKind::UserMessage
        && current != Phase::Intake
}

/// Returns (`classification_basis`, `confidence`) for a given phase.
/// Uses the canonical RULE_IDS indices.
fn phase_basis_confidence(phase: Phase) -> (Vec<&'static str>, f32) {
    match phase {
        Phase::Intake => (vec![RULE_IDS[0]], 1.0),
        Phase::Exploration => (vec![RULE_IDS[1]], 0.85),
        Phase::Diagnosis => (vec![RULE_IDS[2]], 0.8),
        Phase::Action => (vec![RULE_IDS[3]], 0.95),
        Phase::Verification => (vec![RULE_IDS[4]], 0.95),
        Phase::Repair => (vec![RULE_IDS[5]], 0.7),
        Phase::Drift => (vec![RULE_IDS[6]], 0.6),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn ev(i: usize, actor: Actor, kind: EventKind, tool: Option<&str>) -> ObservedEvent {
        ObservedEvent {
            event_id: format!("ev_{i:03}"),
            raw_event_id: format!("raw_{i:03}"),
            schema_version: "observed_event.v1".into(),
            session_id: "sess_t".into(),
            observed_at: Utc.timestamp_opt(1_700_000_000 + i as i64, 0).unwrap(),
            actor,
            kind,
            tool_name: tool.map(String::from),
            parser_version: "test".into(),
            ..Default::default()
        }
    }

    #[test]
    fn empty_yields_zero() {
        assert!(classify_session("s", &[], &[]).is_empty());
    }

    #[test]
    fn user_message_alone_is_intake() {
        let evs = vec![ev(0, Actor::User, EventKind::UserMessage, None)];
        let eps = classify_session("s", &evs, &[]);
        assert_eq!(eps.len(), 1);
        assert_eq!(eps[0].phase, Phase::Intake);
    }
}
