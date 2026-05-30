//! `MissingVerification` L1 extractor (slice-14).
//!
//! Rule (spec §4):
//! For each `action` episode A in a window [intake_i, next_intake_or_end):
//!   1. A must have at least one `diff_hunk` produced inside its event range.
//!   2. There must be NO `verification` episode in the window strictly after A.
//! If both conditions hold, emit a candidate (confidence 0.9).
//!
//! Confidence: fixed 0.9. Promotion: Always.
//! Severity: medium.
//!
//! Evidence refs: action.start_event_id, action.end_event_id,
//!   every hunk's introduced_by_event_id (deduplicated), intake window start_event_id.

use serde_json::json;

use crate::db::repo_episode::EpisodeRow;
use crate::insight::extractor::InsightExtractor;
use crate::insight::types::{FindingCandidate, PromotionPolicy};
use crate::insight::view::SessionInsightView;

pub struct MissingVerification;

impl InsightExtractor for MissingVerification {
    fn category(&self) -> &'static str {
        "missing_verification"
    }

    fn floor(&self) -> f32 {
        0.9
    }

    fn promotion_policy(&self) -> PromotionPolicy {
        PromotionPolicy::Always
    }

    fn extract(&self, view: &SessionInsightView<'_>) -> Vec<FindingCandidate> {
        let episodes = view.episodes;
        if episodes.is_empty() {
            return vec![];
        }

        // Build intake windows: each window is [intake_i.start_event_id, next_intake.start_event_id)
        // Windows are represented by the index range of episodes.
        let mut candidates = Vec::new();

        // Collect intake boundary indices (episodes whose phase == "intake").
        let intake_indices: Vec<usize> = episodes
            .iter()
            .enumerate()
            .filter(|(_, ep)| ep.phase == "intake")
            .map(|(i, _)| i)
            .collect();

        // For each window defined by consecutive intakes (or end of episodes):
        for window_idx in 0..intake_indices.len() {
            let window_start = intake_indices[window_idx];
            let window_end = if window_idx + 1 < intake_indices.len() {
                intake_indices[window_idx + 1]
            } else {
                episodes.len()
            };

            let window_episodes = &episodes[window_start..window_end];
            let intake_ep = &window_episodes[0]; // the intake that opens this window

            // Find all action episodes in this window
            let action_eps: Vec<&EpisodeRow> = window_episodes
                .iter()
                .filter(|ep| ep.phase == "action")
                .collect();

            if action_eps.is_empty() {
                continue;
            }

            // Check whether a verification episode exists in this window
            let has_verification = window_episodes.iter().any(|ep| ep.phase == "verification");

            for action_ep in action_eps {
                // Condition 1: must have at least one diff_hunk introduced inside
                // the action episode's event range.
                // We use the event_id ordering (lexicographic by id) as a proxy
                // for temporal ordering (event_ids are derived from transcript
                // position). The action episode's start_event_id and end_event_id
                // delimit the action range.
                let hunks_in_action: Vec<_> = view
                    .diff_hunks
                    .iter()
                    .filter(|h| {
                        h.session_id == view.session_id
                            && hunk_in_episode_range(
                                &h.introduced_by_event_id,
                                &action_ep.start_event_id,
                                &action_ep.end_event_id,
                                view,
                            )
                    })
                    .collect();

                if hunks_in_action.is_empty() {
                    // No diff_hunk in this action episode → rule does not fire.
                    continue;
                }

                // Condition 2: NO verification episode in the window after this action.
                if has_verification {
                    // A verification exists somewhere in the window → rule does not fire,
                    // even if the verification failed (spec §4 edge cases).
                    continue;
                }

                // Build evidence_refs: action start + end + hunk event ids + intake window start
                let mut evidence_refs: Vec<String> = Vec::new();
                evidence_refs.push(action_ep.start_event_id.clone());
                if action_ep.end_event_id != action_ep.start_event_id {
                    evidence_refs.push(action_ep.end_event_id.clone());
                }
                for h in &hunks_in_action {
                    let eid = &h.introduced_by_event_id;
                    if !evidence_refs.contains(eid) {
                        evidence_refs.push(eid.clone());
                    }
                }
                if !evidence_refs.contains(&intake_ep.start_event_id) {
                    evidence_refs.push(intake_ep.start_event_id.clone());
                }

                let summary = format!(
                    "Action episode {} had no following verification in intake window {}.",
                    action_ep.episode_id, intake_ep.episode_id
                );

                let projection = json!({
                    "category": "missing_verification",
                    "session_id": view.session_id,
                    "action_episode_id": action_ep.episode_id,
                    "introduced_diff_hunks": hunks_in_action.iter().map(|h| &h.diff_hunk_id).collect::<Vec<_>>(),
                    "intake_window": {
                        "start_event_id": intake_ep.start_event_id,
                        "end_event_id": intake_ep.end_event_id,
                    }
                });

                candidates.push(FindingCandidate {
                    category: "missing_verification",
                    subkind: None,
                    confidence_l1: 0.9,
                    severity: "medium",
                    summary,
                    evidence_refs,
                    evidence_projection: projection,
                });
            }
        }

        candidates
    }
}

/// Returns true if `hunk_event_id` falls within the episode's event range.
/// Uses positional lookup in `view.events` to compare ordering.
/// Falls back to lexicographic ordering if events are not found (safe conservative approach).
fn hunk_in_episode_range(
    hunk_event_id: &str,
    episode_start: &str,
    episode_end: &str,
    view: &SessionInsightView<'_>,
) -> bool {
    // Build position lookup if needed (linear scan — acceptable for MVP scale).
    let pos = |id: &str| -> Option<usize> {
        view.events.iter().position(|e| e.event_id == id)
    };

    let hunk_pos = pos(hunk_event_id);
    let start_pos = pos(episode_start);
    let end_pos = pos(episode_end);

    match (hunk_pos, start_pos, end_pos) {
        (Some(h), Some(s), Some(e)) => h >= s && h <= e,
        // Fallback: use string comparison (event_ids include ordinal prefixes)
        _ => hunk_event_id >= episode_start && hunk_event_id <= episode_end,
    }
}
