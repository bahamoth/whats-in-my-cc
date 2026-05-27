//! `caused_repair@v1` — error tool_call → next overlapping repair tool_call.
//!
//! # Frozen thresholds (DEV-S13-02 — never edit in place; create _v2.rs)
//! - `N = 60` s: maximum gap between the error event and the repair tool_call.
//! - `K = 2`  : minimum shared tokens (after stop-word removal) for the rule to fire.
//!
//! # Algorithm
//! 1. Collect all tool_call nodes that have a merged result whose payload
//!    contains `"is_error": true`.
//! 2. For each such error node, find the **next** tool_call node (by `started_at`)
//!    within N seconds whose payload (the "input" field) shares ≥ K tokens with
//!    the error text extracted from the result.
//! 3. Confidence = 0.7 × overlap_score + 0.3 × time_decay, where:
//!    - overlap_score = Jaccard(error_tokens ∩ repair_tokens) / |union|
//!    - time_decay    = 1.0 − delta_seconds / N  (linear, clamped [0,1])
//!
//! DEV-S13-03: tokenisation is lexical (regex `[A-Za-z_][A-Za-z0-9_]+`), not
//! semantic. Minimum token length 4 (avoids single-char false positives).

use std::collections::HashSet;

use serde_json::json;

use crate::ids::derive_edge_id;
use crate::insight::edge_inference::{EdgeInferenceRule, SessionGraphView};
use crate::model::graph::GraphEdge;
use crate::model::meta::SCHEMA_VERSION;

/// Rule ID for this version; frozen — do not change.
pub const RULE_ID: &str = "caused_repair@v1";

/// Maximum seconds between error result and repair call.
const N_SECONDS: i64 = 60;

/// Minimum shared tokens to fire the rule.
const K_MIN_TOKENS: usize = 2;

/// Minimum token length (chars) to avoid single-char noise.
const MIN_TOKEN_LEN: usize = 4;

/// Stop-words excluded from token comparison.
const STOP_WORDS: &[&str] = &[
    "the", "and", "for", "that", "this", "with", "from", "have", "are",
    "was", "were", "been", "will", "would", "could", "should", "into",
    "your", "their", "they", "them", "then", "than", "when", "what",
    "which", "there", "not", "but", "can",
];

pub struct CausedRepairV1;

impl EdgeInferenceRule for CausedRepairV1 {
    fn rule_id(&self) -> &'static str {
        RULE_ID
    }

    fn infer(&self, view: &SessionGraphView<'_>) -> Vec<GraphEdge> {
        let session_id = view.session_id;

        // Collect tool_call nodes ordered by started_at.
        let mut tool_calls: Vec<_> = view
            .nodes
            .iter()
            .filter(|n| n.node_kind == "tool_call")
            .collect();
        tool_calls.sort_by_key(|n| n.started_at);

        let mut edges = Vec::new();

        for (i, error_node) in tool_calls.iter().enumerate() {
            // Check whether this tool_call's merged result is an error.
            let error_text = extract_error_text(&error_node.payload);
            let error_text = match error_text {
                Some(t) if !t.is_empty() => t,
                _ => continue,
            };

            let error_tokens = tokenise(&error_text);
            if error_tokens.is_empty() {
                continue;
            }

            let error_ts = error_node.started_at.timestamp();

            // Look for the next tool_call within N seconds.
            for repair_node in tool_calls.iter().skip(i + 1) {
                let repair_ts = repair_node.started_at.timestamp();
                let delta = repair_ts - error_ts;
                if delta > N_SECONDS {
                    break; // beyond window; nodes are sorted, so we stop.
                }
                if delta < 0 {
                    continue; // shouldn't happen after sort, but be safe.
                }

                // Extract input text from the repair tool_call payload.
                let repair_text = extract_input_text(&repair_node.payload);
                let repair_text = match repair_text {
                    Some(t) if !t.is_empty() => t,
                    _ => continue,
                };

                let repair_tokens = tokenise(&repair_text);
                if repair_tokens.is_empty() {
                    continue;
                }

                let intersection_size = error_tokens.intersection(&repair_tokens).count();
                if intersection_size < K_MIN_TOKENS {
                    continue;
                }

                let union_size = error_tokens.union(&repair_tokens).count();
                let overlap_score = intersection_size as f32 / union_size as f32;
                let time_decay = 1.0f32 - (delta as f32 / N_SECONDS as f32);
                let confidence = (0.7 * overlap_score + 0.3 * time_decay).clamp(0.0, 1.0);

                // Collect the matched terms for attributes.
                let matched_terms: Vec<_> = error_tokens
                    .intersection(&repair_tokens)
                    .cloned()
                    .collect();

                let attrs = json!({
                    "matched_terms": matched_terms,
                    "delta_seconds": delta,
                });

                let edge_id = derive_edge_id(&error_node.node_id, &repair_node.node_id, RULE_ID);
                edges.push(GraphEdge {
                    edge_id,
                    schema_version: SCHEMA_VERSION.into(),
                    session_id: session_id.into(),
                    from_node_id: error_node.node_id.clone(),
                    to_node_id: repair_node.node_id.clone(),
                    edge_kind: "caused_repair".into(),
                    origin: "inferred".into(),
                    attributes: attrs,
                    inference_rule_id: Some(RULE_ID.into()),
                    confidence: Some(confidence),
                });
            }
        }

        edges
    }
}

/// Extract the error text from a tool_call node payload.
/// The merged payload has shape `{ ..call fields.., "result": { "content": [...], "is_error": true } }`.
fn extract_error_text(payload: &serde_json::Value) -> Option<String> {
    let result = payload.get("result")?;
    let is_error = result.get("is_error").and_then(|v| v.as_bool()).unwrap_or(false);
    if !is_error {
        return None;
    }
    // Collect text from all content items.
    let mut parts = Vec::new();
    if let Some(arr) = result.get("content").and_then(|v| v.as_array()) {
        for item in arr {
            if let Some(t) = item.get("text").and_then(|v| v.as_str()) {
                parts.push(t.to_string());
            }
        }
    }
    // Also check a bare "text" or "output" field.
    if parts.is_empty() {
        if let Some(t) = result.get("text").and_then(|v| v.as_str()) {
            parts.push(t.to_string());
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" "))
    }
}

/// Extract text from the tool_call input (the "input" object).
fn extract_input_text(payload: &serde_json::Value) -> Option<String> {
    let input = payload.get("input")?;
    // Gather all string-valued fields in the input object.
    let mut parts = Vec::new();
    if let Some(obj) = input.as_object() {
        for (_k, v) in obj {
            if let Some(s) = v.as_str() {
                parts.push(s.to_string());
            }
        }
    } else if let Some(s) = input.as_str() {
        parts.push(s.to_string());
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" "))
    }
}

/// Tokenise text into a set of lower-case identifier tokens, excluding stop-words
/// and tokens shorter than MIN_TOKEN_LEN characters.
fn tokenise(text: &str) -> HashSet<String> {
    // Simple regex-free tokeniser: split on non-alphanumeric/underscore chars.
    text.split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|s| s.len() >= MIN_TOKEN_LEN)
        .map(|s| s.to_lowercase())
        .filter(|s| !STOP_WORDS.contains(&s.as_str()))
        .collect()
}
