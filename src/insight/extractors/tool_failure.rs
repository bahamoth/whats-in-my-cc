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

/// Tools whose failures are internal agent auto-retries, not user-visible
/// failures (spec §6.3). `StructuredOutput` is the workflow-subagent schema
/// tool that produces the 1941/1953 retry-cycle noise in session 653ea169.
const INTERNAL_RETRY_TOOLS: &[&str] = &["StructuredOutput"];

/// Substrings (lower-cased compare) that mark a benign non-zero exit: a tool
/// "failure" the user does not care about (grep no-match exit 1, Read of a
/// missing file). Kept deliberately tiny + evidence-anchored, not a blanket
/// "only Bash/Edit/Write count" rule (that would drop real MCP/Task/browser
/// failures, which §6.3 lists among the ~28 user-visible ones).
const BENIGN_EXIT_MARKERS: &[&str] = &[
    "no matches found",     // grep / ripgrep exit 1
    "file does not exist",  // Read tool not-found
    "no such file or directory",
];

/// The class a fired tool_failure falls into. Drives both the persisted
/// `subkind` and the finding `severity` (so internal noise never headlines).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureClass {
    /// Genuine user-facing failure — headline-eligible (severity high).
    UserVisible,
    /// Internal agent auto-retry (e.g. StructuredOutput) — severity info.
    InternalRetry,
    /// Benign non-zero exit (grep no-match, Read not-found) — severity info.
    BenignNonzeroExit,
}

impl FailureClass {
    /// Stable string persisted in `finding.subkind` + evidence_projection.
    pub fn as_str(self) -> &'static str {
        match self {
            FailureClass::UserVisible => "user_visible",
            FailureClass::InternalRetry => "internal_retry",
            FailureClass::BenignNonzeroExit => "benign_nonzero_exit",
        }
    }

    /// Severity for the finding. Only `user_visible` is `high`; the two noise
    /// classes are `info` so a `severity=high` headline never lumps them.
    pub fn severity(self) -> &'static str {
        match self {
            FailureClass::UserVisible => "high",
            FailureClass::InternalRetry | FailureClass::BenignNonzeroExit => "info",
        }
    }
}

/// Classify a fired tool_failure by tool name + error excerpt (spec §6.3).
/// Precedence: internal-retry tool first, then benign-exit markers, else
/// user_visible (conservative — an unrecognised failure is surfaced).
pub fn classify_failure(tool_name: &str, error_excerpt: &str) -> FailureClass {
    if INTERNAL_RETRY_TOOLS.contains(&tool_name) {
        return FailureClass::InternalRetry;
    }
    let lc = error_excerpt.to_ascii_lowercase();
    if BENIGN_EXIT_MARKERS.iter().any(|m| lc.contains(m)) {
        return FailureClass::BenignNonzeroExit;
    }
    FailureClass::UserVisible
}

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
            // Payload structure: {"tool_result": {"is_error": bool, ...}}
            // (verified against real fixture: aac68973-729e-4014-a02b-28a556f5ff29).
            let is_error = ev
                .payload
                .pointer("/tool_result/is_error")
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
                        .pointer("/tool_result/is_error")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
            });

            if retry_succeeded {
                // Retry fixed it — do not emit.
                emitted.insert(tool_use_id.clone());
                continue;
            }

            // Find the paired tool_call event for this tool_use_id.
            // tool_name lives on the call event, not the result event.
            let call_ev = events[..i]
                .iter()
                .rev()
                .find(|ev2| {
                    ev2.kind == EventKind::ToolCall
                        && ev2
                            .tool_use_id
                            .as_deref()
                            .map(|id| id == tool_use_id.as_str())
                            .unwrap_or(false)
                });
            let call_event_id = call_ev.map(|ev2| ev2.event_id.clone());
            let tool_name = call_ev
                .and_then(|ev2| ev2.tool_name.as_deref())
                .unwrap_or("unknown");

            let mut evidence_refs = vec![ev.event_id.clone()];
            if let Some(ref call_id) = call_event_id {
                evidence_refs.push(call_id.clone());
            }

            candidates.push(build_candidate(
                view.session_id,
                &ev.event_id,
                call_event_id.as_deref(),
                tool_name,
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
    // Truncate error content for projection (first 512 chars).
    // Payload: {"tool_result": {"content": "...", "tool_use_id": "...", "is_error": true}}
    let tr = payload.pointer("/tool_result");
    let error_excerpt = tr
        .and_then(|p| p.get("content"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .chars()
        .take(512)
        .collect::<String>();

    let tool_use_id = tr
        .and_then(|p| p.get("tool_use_id"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    let class = classify_failure(tool_name, &error_excerpt);

    let summary = format!(
        "Tool {tool_name} failed with is_error=true (tool_use_id={tool_use_id}, class={}).",
        class.as_str()
    );

    let projection = json!({
        "category": "tool_failure",
        "session_id": session_id,
        "failure_class": class.as_str(),
        "tool_use_id": tool_use_id,
        "tool_name": tool_name,
        "error_excerpt_redacted": error_excerpt,
        "tool_result_event_id": result_event_id,
        "paired_call_event_id": call_event_id,
    });

    FindingCandidate {
        category: "tool_failure",
        subkind: Some(class.as_str()),
        confidence_l1: 1.0,
        severity: class.severity(),
        summary,
        evidence_refs,
        evidence_projection: projection,
    }
}
