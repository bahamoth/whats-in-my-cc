//! `large_output_to_next_action@v1` — oversized tool_result → next assistant_message.
//!
//! # Frozen thresholds (DEV-S13-02)
//! - T = 50 * 1024 bytes (51_200): minimum payload byte size to fire.
//! - confidence = 0.6 + 0.4 × normalise(size / max_session_size), clamped [0.6, 1.0].
//!
//! # Note (DEV-S13-05)
//! This rule only produces an edge — it does NOT create a finding.
//! The `context_bloat` finding category (slice-16) reads these edges as evidence.
//!
//! # Payload size measurement
//! We serialise the `result` sub-object of the merged tool_call payload to JSON
//! and measure byte length. This is an approximation; the real wire size depends
//! on the raw transcript bytes, but those are not available in the graph view.
//! The approximation is sufficient for the threshold decision (DEV-S13-06).

use serde_json::json;

use crate::ids::derive_edge_id;
use crate::insight::edge_inference::{EdgeInferenceRule, SessionGraphView};
use crate::model::graph::GraphEdge;
use crate::model::meta::SCHEMA_VERSION;

/// Rule ID for this version; frozen — do not change.
pub const RULE_ID: &str = "large_output_to_next_action@v1";

/// Byte threshold below which the rule does not fire.
const T_BYTES: usize = 50 * 1024; // 51_200

pub struct LargeOutputToNextActionV1;

impl EdgeInferenceRule for LargeOutputToNextActionV1 {
    fn rule_id(&self) -> &'static str {
        RULE_ID
    }

    fn infer(&self, view: &SessionGraphView<'_>) -> Vec<GraphEdge> {
        let session_id = view.session_id;

        // Collect (tool_call node, payload_size) pairs where payload exceeds T.
        let mut candidates: Vec<(&crate::model::graph::GraphNode, usize)> = view
            .nodes
            .iter()
            .filter(|n| n.node_kind == "tool_call")
            .filter_map(|n| {
                let size = measure_result_size(&n.payload);
                if size >= T_BYTES {
                    Some((n, size))
                } else {
                    None
                }
            })
            .collect();
        candidates.sort_by_key(|(n, _)| n.started_at);

        if candidates.is_empty() {
            return vec![];
        }

        // Compute max observed size for normalisation.
        let max_size = candidates.iter().map(|(_, s)| *s).max().unwrap_or(T_BYTES);

        // Build an ordered list of assistant_message nodes for quick lookup.
        let mut asst_nodes: Vec<_> = view
            .nodes
            .iter()
            .filter(|n| n.node_kind == "assistant_message")
            .collect();
        asst_nodes.sort_by_key(|n| n.started_at);

        let mut edges = Vec::new();

        for (call_node, size) in &candidates {
            // Find the next assistant_message after this tool_call.
            let next_asst = asst_nodes
                .iter()
                .find(|a| a.started_at > call_node.started_at);
            let next_asst = match next_asst {
                Some(a) => a,
                None => continue,
            };

            let normalised = (*size as f32) / (max_size as f32);
            let confidence = (0.6f32 + 0.4 * normalised).clamp(0.6, 1.0);

            let attrs = json!({
                "tool_result_size_bytes": *size as i64,
            });

            let edge_id = derive_edge_id(&call_node.node_id, &next_asst.node_id, RULE_ID);
            edges.push(GraphEdge {
                edge_id,
                schema_version: SCHEMA_VERSION.into(),
                session_id: session_id.into(),
                from_node_id: call_node.node_id.clone(),
                to_node_id: next_asst.node_id.clone(),
                edge_kind: "large_output_to_next_action".into(),
                origin: "inferred".into(),
                attributes: attrs,
                inference_rule_id: Some(RULE_ID.into()),
                confidence: Some(confidence),
            });
        }

        edges
    }
}

/// Measure the byte size of the tool_call result by serialising the "result"
/// sub-object to JSON. For unmerged tool_calls (no result key), returns 0.
fn measure_result_size(payload: &serde_json::Value) -> usize {
    match payload.get("result") {
        Some(result) => result.to_string().len(),
        None => 0,
    }
}
