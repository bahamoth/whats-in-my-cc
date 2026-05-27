//! `triggered_by_user_message@v1` — user_message → next tool_call with no
//! preceding assistant_message text in the same turn.
//!
//! # Frozen thresholds (DEV-S13-02)
//! - confidence = 0.85 (fixed, DEV-S13-04)
//!
//! # Algorithm
//! Walk nodes ordered by started_at. When we see a `user_message` node,
//! look at the next node(s):
//! - If the next node is a `tool_call` (no `assistant_message` intervening),
//!   emit an edge from `user_message → tool_call` with confidence 0.85.
//! - If the next node is an `assistant_message`, do NOT fire (common path
//!   already covered by `message_reply` deterministic edges, DEV-S13-04).
//!
//! We look only at the immediately next non-hook_event, non-otel_span,
//! non-diff_hunk, non-verification_run node (the same filter used by
//! `turn_order` in build.rs).

use serde_json::json;

use crate::ids::derive_edge_id;
use crate::insight::edge_inference::{EdgeInferenceRule, SessionGraphView};
use crate::model::graph::GraphEdge;
use crate::model::meta::SCHEMA_VERSION;

/// Rule ID for this version; frozen — do not change.
pub const RULE_ID: &str = "triggered_by_user_message@v1";

/// Fixed confidence for this rule.
const CONFIDENCE: f32 = 0.85;

pub struct TriggeredByUserMessageV1;

impl EdgeInferenceRule for TriggeredByUserMessageV1 {
    fn rule_id(&self) -> &'static str {
        RULE_ID
    }

    fn infer(&self, view: &SessionGraphView<'_>) -> Vec<GraphEdge> {
        let session_id = view.session_id;

        // Conversation-turn nodes (same exclusion list as turn_order in build.rs).
        let mut ordered: Vec<_> = view
            .nodes
            .iter()
            .filter(|n| {
                !matches!(
                    n.node_kind.as_str(),
                    "otel_span"
                        | "file_event"
                        | "git_commit"
                        | "diff_hunk"
                        | "metric_sample"
                        | "log_record"
                        | "verification_run"
                )
            })
            .collect();
        ordered.sort_by(|a, b| (a.started_at, &a.node_id).cmp(&(b.started_at, &b.node_id)));

        let mut edges = Vec::new();

        for i in 0..ordered.len() {
            let node = ordered[i];
            if node.node_kind != "user_message" {
                continue;
            }
            // Look at the immediately next conversation node.
            if let Some(next) = ordered.get(i + 1) {
                if next.node_kind == "tool_call" {
                    // Fire: no assistant text in between.
                    let edge_id = derive_edge_id(&node.node_id, &next.node_id, RULE_ID);
                    edges.push(GraphEdge {
                        edge_id,
                        schema_version: SCHEMA_VERSION.into(),
                        session_id: session_id.into(),
                        from_node_id: node.node_id.clone(),
                        to_node_id: next.node_id.clone(),
                        edge_kind: "triggered_by_user_message".into(),
                        origin: "inferred".into(),
                        attributes: json!({}),
                        inference_rule_id: Some(RULE_ID.into()),
                        confidence: Some(CONFIDENCE),
                    });
                }
                // If next is assistant_message, do not fire (DEV-S13-04).
            }
        }

        edges
    }
}
