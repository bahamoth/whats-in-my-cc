//! `ToolFailure` L1 extractor (slice-14).
//!
//! Rule (spec §5):
//! For each `tool_result` event with `is_error == true`:
//!   Walk forward in the same session up to M=5 events.
//!   If within that window there is another `tool_result` for the same
//!   `tool_use_id` with `is_error == false`, the rule does NOT fire (retry succeeded).
//!   Otherwise, the rule fires.
//!
//! Confidence: fixed 1.0 (we quote the error flag directly).
//! Severity: high.
//!
//! Evidence refs: [tool_result_event_id, paired_tool_call_event_id] (if call found).

use serde_json::json;

use crate::insight::extractor::InsightExtractor;
use crate::insight::types::{FindingCandidate, PromotionPolicy};
use crate::insight::view::SessionInsightView;
use crate::model::observed::EventKind;

/// Number of events forward to check for a compensating successful retry.
const RETRY_WINDOW: usize = 5;

pub struct ToolFailure;

impl InsightExtractor for ToolFailure {
    fn category(&self) -> &'static str {
        "tool_failure"
    }

    fn floor(&self) -> f32 {
        1.0
    }

    fn promotion_policy(&self) -> PromotionPolicy {
        PromotionPolicy::Always
    }

    fn extract(&self, view: &SessionInsightView<'_>) -> Vec<FindingCandidate> {
        let events = view.events;
        let mut candidates = Vec::new();
        // Track which tool_use_ids we've already emitted a finding for
        // to avoid duplicates when there are multiple error results for the same id.
        let mut emitted: std::collections::HashSet<String> = std::collections::HashSet::new();

        for (i, ev) in events.iter().enumerate() {
            if ev.kind != EventKind::ToolResult {
                continue;
            }

            // Check is_error flag (spec §5 edge case: absent = false, no fire).
            let is_error = ev
                .payload
                .get("is_error")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            if !is_error {
                continue;
            }

            let Some(tool_use_id) = &ev.tool_use_id else {
                // No tool_use_id → cannot correlate with a retry; fire conservatively.
                let evidence_refs = vec![ev.event_id.clone()];
                candidates.push(build_candidate(
                    view.session_id,
                    &ev.event_id,
                    None,
                    ev.tool_name.as_deref().unwrap_or("unknown"),
                    &ev.payload,
                    evidence_refs,
                ));
                continue;
            };

            if emitted.contains(tool_use_id.as_str()) {
                continue;
            }

            // Check forward window for a successful retry with the same tool_use_id.
            let window_end = (i + 1 + RETRY_WINDOW).min(events.len());
            let retry_succeeded = events[i + 1..window_end].iter().any(|ev2| {
                ev2.kind == EventKind::ToolResult
                    && ev2
                        .tool_use_id
                        .as_deref()
                        .map(|id| id == tool_use_id.as_str())
                        .unwrap_or(false)
                    && !ev2
                        .payload
                        .get("is_error")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
            });

            if retry_succeeded {
                // Retry fixed it — do not emit.
                emitted.insert(tool_use_id.clone());
                continue;
            }

            // Find the paired tool_call event for this tool_use_id.
            let call_event_id = events[..i]
                .iter()
                .rev()
                .find(|ev2| {
                    ev2.kind == EventKind::ToolCall
                        && ev2
                            .tool_use_id
                            .as_deref()
                            .map(|id| id == tool_use_id.as_str())
                            .unwrap_or(false)
                })
                .map(|ev2| ev2.event_id.clone());

            let mut evidence_refs = vec![ev.event_id.clone()];
            if let Some(ref call_id) = call_event_id {
                evidence_refs.push(call_id.clone());
            }

            candidates.push(build_candidate(
                view.session_id,
                &ev.event_id,
                call_event_id.as_deref(),
                ev.tool_name.as_deref().unwrap_or("unknown"),
                &ev.payload,
                evidence_refs,
            ));
            emitted.insert(tool_use_id.clone());
        }

        candidates
    }
}

fn build_candidate(
    session_id: &str,
    result_event_id: &str,
    call_event_id: Option<&str>,
    tool_name: &str,
    payload: &serde_json::Value,
    evidence_refs: Vec<String>,
) -> FindingCandidate {
    // Truncate error content for projection (first 512 bytes).
    let error_excerpt = payload
        .get("content")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .chars()
        .take(512)
        .collect::<String>();

    let tool_use_id = payload
        .get("tool_use_id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    let summary = format!("Tool {tool_name} failed with is_error=true (tool_use_id={tool_use_id}).");

    let projection = json!({
        "category": "tool_failure",
        "session_id": session_id,
        "tool_use_id": tool_use_id,
        "tool_name": tool_name,
        "error_excerpt_redacted": error_excerpt,
        "tool_result_event_id": result_event_id,
        "paired_call_event_id": call_event_id,
    });

    FindingCandidate {
        category: "tool_failure",
        confidence_l1: 1.0,
        severity: "high",
        summary,
        evidence_refs,
        evidence_projection: projection,
    }
}
