//! `FinalStateMismatch` L1 extractor (slice-16).
//!
//! Rule (spec §5):
//! Fires when ALL of:
//! 1. The opening `user_message` contains a goal verb from the frozen lexicon.
//! 2. The session ends with a `verification_run` whose `status == "failed"`, OR
//!    the final `assistant_message` does NOT contain explicit completion markers.
//! 3. The final `assistant_message` does not contain completion markers.
//!
//! Emits at most ONE finding per session (session-level grain, DEV-S16-06).
//!
//! L1 confidence: 0.6 — promotes directly (deterministic L1).
//! Severity: medium.
//!
//! Goal verb lexicon and completion marker lexicon are frozen (DEV-S16-03).

use serde_json::json;

use crate::insight::extractor::InsightExtractor;
use crate::insight::redaction_shim;
use crate::insight::types::FindingCandidate;
use crate::insight::view::SessionInsightView;
use crate::model::observed::{Actor, EventKind};

/// Goal verbs that indicate the user had an explicit objective (spec §5, frozen).
pub const GOAL_VERBS: &[&str] = &[
    "fix",
    "add",
    "remove",
    "delete",
    "make",
    "implement",
    "rewrite",
    "refactor",
    "improve",
    "speed up",
    "optimise",
    "optimize",
];

/// Completion markers in the final assistant message — if any present, no fire (spec §5).
pub const COMPLETION_MARKERS: &[&str] = &[
    "done",
    "complete",
    "completed",
    "all tests pass",
    "tests pass",
    "fixed",
    "resolved",
    "finished",
    "successfully",
];

pub struct FinalStateMismatch;

impl InsightExtractor for FinalStateMismatch {
    fn category(&self) -> &'static str {
        "final_state_mismatch"
    }

    fn floor(&self) -> f32 {
        0.6
    }

    fn extract(&self, view: &SessionInsightView<'_>) -> Vec<FindingCandidate> {
        let events = view.events;
        if events.is_empty() {
            return vec![];
        }

        // --- Condition 1: find the first user_message with a goal verb ---
        let goal_result = events.iter().find_map(|ev| {
            if ev.actor != Actor::User || ev.kind != EventKind::UserMessage {
                return None;
            }
            let text = extract_message_text(&ev.payload);
            let matched = find_goal_verbs(&text);
            if matched.is_empty() {
                None
            } else {
                Some((ev.event_id.clone(), matched, text))
            }
        });

        let Some((goal_event_id, matched_verbs, goal_text)) = goal_result else {
            return vec![];
        };

        // --- Condition 3: find the final assistant_message and check for completion markers ---
        let last_assistant = events
            .iter()
            .rev()
            .find(|ev| ev.actor == Actor::Assistant && ev.kind == EventKind::AssistantMessage);

        let (last_assistant_event_id, last_assistant_text) = match last_assistant {
            Some(ev) => {
                let text = extract_message_text(&ev.payload);
                (ev.event_id.clone(), text)
            }
            None => (String::new(), String::new()),
        };

        let has_completion_marker = has_any_completion_marker(&last_assistant_text);
        if has_completion_marker {
            return vec![];
        }

        // --- Condition 2: final verification run status, OR trailing tool failure ---
        let last_verification = view.verification_runs.last();
        let final_verification_failed = last_verification
            .map(|vr| vr.status == "failed" || vr.status == "error")
            .unwrap_or(false);

        // If no verification run at all AND no completion marker, that's ambiguous.
        // Only fire if we have positive evidence of failure (failed verification).
        if !final_verification_failed {
            return vec![];
        }

        // Build projection
        let goal_excerpt = redaction_shim::apply_text_truncated(&goal_text, 512);
        let last_asst_excerpt = redaction_shim::apply_text_truncated(&last_assistant_text, 1024);

        let last_vr_proj = last_verification.map(|vr| {
            let fs = vr.failure_summary.as_deref().unwrap_or("");
            let fs_redacted = redaction_shim::apply_text_truncated(fs, 256);
            json!({
                "verification_run_id": vr.verification_run_id,
                "status": vr.status,
                "failure_summary_redacted": fs_redacted
            })
        });

        let projection = json!({
            "category": "final_state_mismatch",
            "session_id": view.session_id,
            "goal": {
                "user_message_event_id": goal_event_id,
                "matched_verbs": matched_verbs,
                "excerpt_redacted": goal_excerpt
            },
            "final_state": {
                "last_assistant_message_event_id": last_assistant_event_id,
                "last_assistant_excerpt_redacted": last_asst_excerpt,
                "last_verification_run": last_vr_proj,
                "trailing_tool_failure": null
            }
        });

        let vr_id = last_verification
            .map(|vr| vr.verification_run_id.as_str())
            .unwrap_or("none");
        let summary = format!(
            "User goal (verbs: {}) not corroborated: last verification {} failed.",
            matched_verbs.join(", "),
            vr_id
        );

        let mut evidence_refs = vec![goal_event_id.clone()];
        if !last_assistant_event_id.is_empty() {
            evidence_refs.push(last_assistant_event_id.clone());
        }
        if let Some(vr) = last_verification {
            evidence_refs.push(vr.trigger_event_id.clone());
        }

        // Session-level grain: at most one finding.
        vec![FindingCandidate {
            category: "final_state_mismatch",
            subkind: None,
            confidence_l1: 0.6,
            severity: "medium",
            summary,
            evidence_refs,
            evidence_projection: projection,
        }]
    }
}

/// Find all goal verbs present in `text` (case-insensitive, word-boundary aware).
fn find_goal_verbs(text: &str) -> Vec<String> {
    let lower = text.to_lowercase();
    GOAL_VERBS
        .iter()
        .filter(|&&verb| {
            // Word-boundary check: surrounded by non-alpha or at start/end.
            let mut pos = 0;
            while let Some(idx) = lower[pos..].find(verb) {
                let abs = pos + idx;
                let before_ok = abs == 0
                    || !lower.chars().nth(abs - 1).map(|c| c.is_alphabetic()).unwrap_or(false);
                let after_ok = abs + verb.len() >= lower.len()
                    || !lower
                        .chars()
                        .nth(abs + verb.len())
                        .map(|c| c.is_alphabetic())
                        .unwrap_or(false);
                if before_ok && after_ok {
                    return true;
                }
                pos = abs + 1;
            }
            false
        })
        .map(|s| s.to_string())
        .collect()
}

/// Check if `text` (lowercased) contains any completion marker.
fn has_any_completion_marker(text: &str) -> bool {
    let lower = text.to_lowercase();
    COMPLETION_MARKERS.iter().any(|&marker| lower.contains(marker))
}

/// Extract plain text from a message's content array.
fn extract_message_text(payload: &serde_json::Value) -> String {
    if let Some(content) = payload.pointer("/message/content") {
        if let Some(arr) = content.as_array() {
            return arr
                .iter()
                .filter_map(|item| item.get("text").and_then(|t| t.as_str()).map(str::to_string))
                .collect::<Vec<_>>()
                .join(" ");
        }
        if let Some(s) = content.as_str() {
            return s.to_string();
        }
    }
    String::new()
}
