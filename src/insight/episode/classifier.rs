//! Episode phase classifier — state machine (slice-12).
//!
//! `classify_session` is a pure function: same input always produces the same
//! output (`tests/episode_determinism.rs` asserts this). No I/O, no globals.
//!
//! # Algorithm
//!
//! Left-to-right pass over the ordered event stream. At each position the
//! state machine consults only backward state (recent exploration streak,
//! had_failure flag, last_error_at / last_verification_at) to decide whether
//! to:
//!   (a) continue the current episode, or
//!   (b) emit a boundary and start a new episode.
//!
//! DEV-S12-02 (revised): the spec originally called for a 3-event lookahead
//! window. Implementation converged on backward-only state instead; the
//! lookahead helper was removed when the dead-code warning surfaced and the
//! deviation was rewritten to reflect actual behaviour.
//!
//! # Phase transition table
//!
//! | Event signal | New phase |
//! |---|---|
//! | user_message from User actor | Intake |
//! | read-only tool_call, after recent error | Diagnosis |
//! | read-only tool_call | Exploration |
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

// --- helpers -----------------------------------------------------------------

/// Read-only tool names (Bash excluded — Bash is mutating unless its command
/// is on the verification allowlist, which the classifier does not see here.
/// VerificationRun row injection handles the verification case separately).
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

/// First-token allowlist of read-only-by-default shell commands (matches the
/// design spec §B1 line 22). `sed`/`awk` are deliberately excluded: they
/// commonly mutate via `-i`, and per the conservative-default they err toward
/// Action. `find`/`sort` stay here but are forced to mutation by the
/// `-delete` / ` -o ` markers when they actually write.
const BASH_READ_ONLY_FIRST_TOKENS: &[&str] = &[
    "grep", "rg", "egrep", "fgrep", "ls", "cat", "find", "head", "tail", "wc",
    "which", "pwd", "echo", "env", "file", "stat", "du", "df", "tree", "sort",
    "uniq",
];

/// Read-only `git` subcommands (used only when the first token is `git`).
const GIT_READ_ONLY_SUBCOMMANDS: &[&str] = &[
    "status", "log", "diff", "show", "branch", "blame", "rev-parse", "describe",
];

/// Substrings whose presence forces a Bash command to be treated as mutating,
/// even if its first token is on the read-only allowlist. Covers output
/// redirection, compound/sequenced commands (ambiguous → mutation), explicit
/// mutating utilities, package/build tooling, and write flags on otherwise
/// read-only utilities (`find -delete`, `sort -o`, in-place `-i`). Note that
/// ` -o ` also catches `find ... -o ...` (logical OR) — over-classifying a
/// read-only find as Action is the safe direction (conservative default).
const BASH_MUTATING_MARKERS: &[&str] = &[
    ">", ">>", "rm ", "mv ", "cp ", "mkdir", "touch", "tee", "&&", ";", "||",
    "-delete", " -o ", "-i ", "-i'",
    "git commit", "git push", "git add", "git reset", "git checkout", "git rm",
    "npm", "pnpm", "yarn", "cargo build", "cargo run", "cargo test",
];

/// Heuristic: decide whether a `Bash` command is *unambiguously* read-only.
///
/// HEURISTIC — documented in the Slice 3 design spec
/// `docs/superpowers/specs/2026-05-31-episode-redesign-slice3-design.md`
/// §B1/§D2 (autonomous decision flag #2). Shell parsing is inherently
/// incomplete (pipes, subshells, variable expansion, aliases, novel write
/// flags), so the rule **errs toward mutation**: it returns `true` only for a
/// narrow, recognized read-only shape and `false` for everything else,
/// including commands it cannot confidently parse. This is a best-effort
/// denylist, NOT a soundness guarantee — a sufficiently exotic mutating
/// command with a read-only first token and no recognized marker could slip
/// through; the marker set below is widened as such cases surface (e.g. the
/// review that added `-delete` / ` -o ` / `-i`).
///
/// A command is classified read-only iff:
///   1. its first token is on [`BASH_READ_ONLY_FIRST_TOKENS`], OR it is
///      `git <read-only-subcommand>` per [`GIT_READ_ONLY_SUBCOMMANDS`], AND
///   2. it contains none of [`BASH_MUTATING_MARKERS`] (redirection, compound
///      operators, mutating utilities, package/build tools, write flags).
///
/// Anything else → `false` (treated as mutation).
///
/// Note: VerificationRun handling (Bash-on-verify-allowlist → Verification) is
/// orthogonal and happens before this is consulted; `cargo test` etc. are in
/// the mutating-marker denylist here so they are never mislabeled read-only.
fn bash_is_read_only(command: &str) -> bool {
    let cmd = command.trim();
    if cmd.is_empty() {
        return false;
    }
    // Any mutating marker anywhere → not read-only.
    if BASH_MUTATING_MARKERS.iter().any(|m| cmd.contains(m)) {
        return false;
    }
    let mut tokens = cmd.split_whitespace();
    let first = match tokens.next() {
        Some(t) => t,
        None => return false,
    };
    if first == "git" {
        return tokens
            .next()
            .map(|sub| GIT_READ_ONLY_SUBCOMMANDS.contains(&sub))
            .unwrap_or(false);
    }
    BASH_READ_ONLY_FIRST_TOKENS.contains(&first)
}

/// Extract the Bash command string from a tool_call payload. The ingest mapping
/// builds tool_call payloads as `{"tool_name":..,"input":{"command":..}}`, so
/// the command lives at `/input/command`.
fn bash_command(ev: &ObservedEvent) -> Option<&str> {
    ev.payload.pointer("/input/command").and_then(|v| v.as_str())
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

/// Emit a completed episode span covering events `[start_idx ..= end_idx]`.
/// `evidence_node_ids` is populated with the spanned event_ids — the episode's
/// evidence is the events it covers (the classifier owns event_ids; it has no
/// access to derived graph node_ids, and reaching into the graph would break
/// the pure-function determinism contract). The graph builder serializes this
/// vec verbatim into the `episode.evidence_node_ids` JSON column (build.rs).
fn emit(
    session_id: &str,
    phase: Phase,
    events: &[ObservedEvent],
    start_idx: usize,
    end_idx: usize,
    basis: Vec<&'static str>,
    confidence: f32,
) -> EpisodeRecord {
    let start = &events[start_idx];
    let end = &events[end_idx];
    let episode_id = make_episode_id(session_id, phase, &start.event_id, &end.event_id);
    let evidence_node_ids: Vec<String> = events[start_idx..=end_idx]
        .iter()
        .map(|e| e.event_id.clone())
        .collect();
    EpisodeRecord {
        episode_id,
        schema_version: "episode.v1".into(),
        session_id: session_id.to_string(),
        phase,
        start_event_id: start.event_id.clone(),
        end_event_id: end.event_id.clone(),
        started_at: start.observed_at,
        ended_at: end.observed_at,
        evidence_node_ids,
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
            // Emit the current episode [phase_start_idx ..= i-1]. The new phase
            // starts at i — ranges never overlap (prev ends at i-1).
            let (basis, confidence) = phase_basis_confidence(st.current_phase);
            out.push(emit(
                session_id,
                st.current_phase,
                events,
                st.phase_start_idx,
                i - 1,
                basis,
                confidence,
            ));

            // Start new episode at i.
            st.current_phase = new_phase;
            st.phase_start_idx = i;
            st.reset_streak();
        }

        // Exploration streak tracking for drift.
        if st.current_phase == Phase::Exploration {
            st.exploration_streak += 1;
            if st.exploration_streak >= DRIFT_THRESHOLD {
                // Close the Exploration episode at i-1 (NOT i) and begin Drift
                // at i — same off-by-one as the normal boundary, so events[i]
                // belongs to exactly one episode. Pre-fix this ended at i and
                // started Drift at i, double-classifying events[i] (spec §6.4:
                // 513 shared event_ids / zero-duration rows in 653ea169).
                let (basis, confidence) = phase_basis_confidence(Phase::Exploration);
                out.push(emit(
                    session_id,
                    Phase::Exploration,
                    events,
                    st.phase_start_idx,
                    i - 1,
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

    // Emit the final episode [phase_start_idx ..= last].
    let (basis, confidence) = phase_basis_confidence(st.current_phase);
    out.push(emit(
        session_id,
        st.current_phase,
        events,
        st.phase_start_idx,
        events.len() - 1,
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
            // Bash is in MUTATION_TOOLS by default, but a command that is
            // unambiguously read-only (Slice 3 B1 heuristic) should follow the
            // read-only path instead of being mislabeled Action/Repair. The
            // conservative default (ambiguous → mutation) is enforced by
            // `bash_is_read_only` returning false when unsure.
            let bash_read_only =
                tool == "Bash" && bash_command(ev).map(bash_is_read_only).unwrap_or(false);

            if is_mutation_tool(tool) && !bash_read_only {
                // Repair if we had a recent failure or failed verification.
                if st.had_failure {
                    return Phase::Repair;
                }
                return Phase::Action;
            }
            if is_read_only_tool(tool) || bash_read_only {
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

    /// Bash tool_call event with the given command at the canonical payload
    /// path `/input/command` (matches `ingest::mapping`, which builds
    /// `{"tool_name":..,"input":{"command":..}}`).
    fn bash_call(i: usize, command: &str) -> ObservedEvent {
        let mut e = ev(i, Actor::Assistant, EventKind::ToolCall, Some("Bash"));
        e.payload = serde_json::json!({
            "tool_name": "Bash",
            "input": { "command": command }
        });
        e
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

    // --- Slice 3 B1: read-only Bash classification ---------------------------

    /// The phase that the tool_call at index 1 (after an Intake user_message at
    /// index 0) gets classified into.
    fn phase_of_call(call: ObservedEvent) -> Phase {
        let evs = vec![ev(0, Actor::User, EventKind::UserMessage, None), call];
        let eps = classify_session("s", &evs, &[]);
        // Last episode covers the tool_call (Intake is emitted first).
        eps.last().unwrap().phase
    }

    #[test]
    fn read_only_bash_grep_is_exploration_not_action() {
        // `grep -n foo src/` is unambiguously read-only → Exploration, not Action.
        assert_eq!(phase_of_call(bash_call(1, "grep -n foo src/")), Phase::Exploration);
    }

    #[test]
    fn mutating_bash_rm_is_action() {
        assert_eq!(phase_of_call(bash_call(1, "rm -rf build")), Phase::Action);
    }

    #[test]
    fn compound_bash_is_action_conservative() {
        // Compound command (`&&`) is ambiguous → conservative mutation default.
        assert_eq!(phase_of_call(bash_call(1, "cat x && rm y")), Phase::Action);
    }

    #[test]
    fn git_status_bash_is_read_only() {
        assert_eq!(phase_of_call(bash_call(1, "git status")), Phase::Exploration);
    }

    #[test]
    fn git_commit_bash_is_action() {
        assert_eq!(phase_of_call(bash_call(1, "git commit -m x")), Phase::Action);
    }

    #[test]
    fn non_bash_tools_unchanged() {
        // Edit is mutation → Action; Read is read-only → Exploration. The Bash
        // command inspection must not alter behaviour for non-Bash tools.
        assert_eq!(
            phase_of_call(ev(1, Actor::Assistant, EventKind::ToolCall, Some("Edit"))),
            Phase::Action
        );
        assert_eq!(
            phase_of_call(ev(1, Actor::Assistant, EventKind::ToolCall, Some("Read"))),
            Phase::Exploration
        );
    }

    #[test]
    fn read_only_bash_after_error_is_diagnosis() {
        // Read-only Bash should follow the read-only path, which routes to
        // Diagnosis when there is a recent error (same as Read/Grep).
        let evs = vec![
            ev(0, Actor::User, EventKind::UserMessage, None),
            ev(1, Actor::Assistant, EventKind::ToolCall, Some("Edit")),
            {
                let mut e = ev(2, Actor::Tool, EventKind::ToolResult, Some("Edit"));
                e.payload = serde_json::json!({"is_error": true});
                e
            },
            bash_call(3, "grep -n boom src/"),
        ];
        let eps = classify_session("s", &evs, &[]);
        assert!(
            eps.iter().any(|e| e.phase == Phase::Diagnosis),
            "expected diagnosis for read-only Bash after error; got {:?}",
            eps.iter().map(|e| e.phase).collect::<Vec<_>>()
        );
    }

    /// Conservative-invariant table: commands that mutate (even with a
    /// read-only first token) MUST classify as Action; clearly read-only
    /// commands stay Exploration. Locks the review finding (sed -i / find
    /// -delete / sort -o were false-positive read-only).
    #[test]
    fn bash_conservative_invariant_table() {
        let cases: &[(&str, Phase)] = &[
            // Mutating despite read-only-looking first token → Action.
            ("sed -i 's/a/b/' f", Phase::Action),
            ("find . -name x -delete", Phase::Action),
            ("sort f -o f", Phase::Action),
            // Clearly read-only → Exploration.
            ("grep -n foo src/", Phase::Exploration),
            ("ls", Phase::Exploration),
            ("cat f", Phase::Exploration),
            ("git status", Phase::Exploration),
            ("find . -name x", Phase::Exploration),
        ];
        for (cmd, want) in cases {
            assert_eq!(
                phase_of_call(bash_call(1, cmd)),
                *want,
                "command {cmd:?} expected {want:?}"
            );
        }
    }
}
