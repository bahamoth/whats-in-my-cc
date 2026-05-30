//! `ContextBloat` L1+L2 extractor (slice-16).
//!
//! Rule (spec §4):
//! Fires when ALL of:
//! 1. A `tool_result.payload` whose serialised content size > T = 50 * 1024 bytes.
//! 2. The next `assistant_message` (within M = 3 events) exists.
//! 3. There is NO later `tool_call` within M events that references content from the
//!    bloated `tool_result` (lexical overlap of ≥ 3 stems).
//!
//! L1 confidence: 0.5. Promotion: IfAbove(1.0) — always judge.
//! Severity: low.
//!
//! Thresholds T=50KB, M=3, overlap≥3 are constants (DEV-S16-02).

use serde_json::json;

use crate::insight::extractor::InsightExtractor;
use crate::insight::redaction_shim;
use crate::insight::types::{FindingCandidate, PromotionPolicy};
use crate::insight::view::SessionInsightView;
use crate::model::observed::EventKind;

/// Payload size threshold in bytes (spec §4).
pub const BLOAT_THRESHOLD_BYTES: usize = 50 * 1024;

/// Number of events forward to look for the next assistant_message (spec §4).
pub const NEXT_EVENT_WINDOW: usize = 3;

/// Minimum number of stem matches to consider the bloat "reused" downstream.
pub const MIN_OVERLAP_STEMS: usize = 3;

pub struct ContextBloat;

impl InsightExtractor for ContextBloat {
    fn category(&self) -> &'static str {
        "context_bloat"
    }

    fn floor(&self) -> f32 {
        0.5
    }

    fn promotion_policy(&self) -> PromotionPolicy {
        PromotionPolicy::IfAbove(1.0)
    }

    fn extract(&self, view: &SessionInsightView<'_>) -> Vec<FindingCandidate> {
        let events = view.events;
        let mut candidates: Vec<FindingCandidate> = Vec::new();

        for (i, ev) in events.iter().enumerate() {
            if ev.kind != EventKind::ToolResult {
                continue;
            }

            // Compute payload content size from tool_result content field.
            let content = ev
                .payload
                .pointer("/tool_result/content")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let content_size = content.len();

            if content_size <= BLOAT_THRESHOLD_BYTES {
                continue;
            }

            // Condition 2: find the next assistant_message within M events.
            let window_end = (i + 1 + NEXT_EVENT_WINDOW).min(events.len());
            let next_assistant = events[i + 1..window_end]
                .iter()
                .enumerate()
                .find(|(_, ev2)| ev2.kind == EventKind::AssistantMessage)
                .map(|(offset, ev2)| (i + 1 + offset, ev2));

            let Some((asst_idx, asst_ev)) = next_assistant else {
                // No assistant message in next M events — spec says no fire.
                continue;
            };

            // Condition 3: check downstream lexical overlap.
            let bloat_stems = extract_stems(content);
            let next_end = (asst_idx + 1 + NEXT_EVENT_WINDOW).min(events.len());
            let downstream_tool_inputs: Vec<String> = events[asst_idx + 1..next_end]
                .iter()
                .filter(|ev2| ev2.kind == EventKind::ToolCall)
                .map(|ev2| {
                    ev2.payload
                        .pointer("/tool_use/input/command")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string()
                })
                .collect();

            let overlap_count = count_stem_overlap(&bloat_stems, &downstream_tool_inputs);

            if overlap_count >= MIN_OVERLAP_STEMS {
                // Bloat was reused — do not fire.
                continue;
            }

            // Build projection
            let payload_excerpt = redaction_shim::apply_text_truncated(content, 512);
            let payload_tail = if content.len() > 256 {
                redaction_shim::apply_text_truncated(&content[content.len() - 256..], 256)
            } else {
                String::new()
            };

            let asst_text = extract_assistant_text(&asst_ev.payload);
            let asst_excerpt = redaction_shim::apply_text_truncated(&asst_text, 512);
            let asst_token_estimate = estimate_tokens(&asst_text);

            let downstream_excerpts: Vec<serde_json::Value> = downstream_tool_inputs
                .iter()
                .map(|inp| {
                    let redacted = redaction_shim::apply_text_truncated(inp, 256);
                    serde_json::Value::String(redacted)
                })
                .collect();

            let projection = json!({
                "category": "context_bloat",
                "session_id": view.session_id,
                "tool_result": {
                    "event_id": ev.event_id,
                    "tool_name": ev.tool_name,
                    "payload_size_bytes": content_size,
                    "payload_excerpt_redacted": payload_excerpt,
                    "payload_tail_excerpt_redacted": payload_tail
                },
                "next_assistant": {
                    "event_id": asst_ev.event_id,
                    "estimated_tokens": asst_token_estimate,
                    "excerpt_redacted": asst_excerpt
                },
                "downstream_usage_signal": {
                    "lexical_overlap_with_next_tool_calls": overlap_count,
                    "next_three_tool_call_inputs_redacted": downstream_excerpts
                }
            });

            let summary = format!(
                "Large tool_result ({content_size} bytes) from {:?} not reused in subsequent turn.",
                ev.tool_name.as_deref().unwrap_or("unknown")
            );

            candidates.push(FindingCandidate {
                category: "context_bloat",
                subkind: None,
                confidence_l1: 0.5,
                severity: "low",
                summary,
                evidence_refs: vec![ev.event_id.clone(), asst_ev.event_id.clone()],
                evidence_projection: projection,
            });
        }

        candidates
    }
}

/// Extract the unique "stems" from text: lowercase words ≥ 4 chars that look
/// like identifiers or notable nouns (heuristic per spec §4 clause 3).
fn extract_stems(text: &str) -> std::collections::HashSet<String> {
    text.split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|s| s.len() >= 4)
        .map(|s| s.to_lowercase())
        .collect()
}

/// Count how many stems from the bloat appear in any of the downstream inputs.
fn count_stem_overlap(
    bloat_stems: &std::collections::HashSet<String>,
    downstream_inputs: &[String],
) -> usize {
    let combined: std::collections::HashSet<String> = downstream_inputs
        .iter()
        .flat_map(|inp| extract_stems(inp))
        .collect();
    bloat_stems.intersection(&combined).count()
}

/// Extract plain text from an assistant message's content array.
fn extract_assistant_text(payload: &serde_json::Value) -> String {
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

/// Rough token estimate: 1 token ≈ 4 chars (heuristic, spec §4 condition 2).
fn estimate_tokens(text: &str) -> usize {
    text.len() / 4
}
