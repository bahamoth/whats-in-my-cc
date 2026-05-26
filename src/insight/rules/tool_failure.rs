//! Slice-11 — `tool_failure` rule.
//!
//! Trigger: an ObservedEvent of kind `tool_result` whose payload has
//! `tool_result.is_error == true`. Evidence: the graph node whose
//! `source_event_ids` contains the failing tool_result's event_id (in the
//! current builder this is the merged `tool_call` node).
//!
//! Real-data anchoring: `is_error:true` is present in real Claude Code
//! transcripts (verified in slice-11 design doc — `implementation-notes`
//! §32). Sub-classification (user-rejection vs tool-error) is deferred to a
//! later slice; this rule fires on the bare flag.

use serde_json::json;

use super::Rule;
use crate::db::repo_finding::NewFinding;
use crate::ids::derive_node_id;
use crate::model::graph::GraphNode;
use crate::model::observed::{EventKind, ObservedEvent};

pub struct ToolFailureRule;

const RULE_VERSION: &str = "tool_failure.v1";
const SCHEMA_VERSION: &str = "finding.v1";
const CATEGORY: &str = "tool_failure";
const SEVERITY: &str = "medium";
const CONFIDENCE: f64 = 0.95;
const CLAIM: &str = "A tool result reported an error (is_error=true).";

impl Rule for ToolFailureRule {
    fn name(&self) -> &'static str {
        "tool_failure"
    }

    fn evaluate(
        &self,
        session_id: &str,
        events: &[ObservedEvent],
        graph_nodes: &[GraphNode],
        generated_at: &str,
    ) -> Vec<NewFinding> {
        let mut out = Vec::new();
        for ev in events {
            if ev.kind != EventKind::ToolResult {
                continue;
            }
            let is_error = ev
                .payload
                .get("tool_result")
                .and_then(|tr| tr.get("is_error"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if !is_error {
                continue;
            }
            // Evidence: the graph node containing this event_id. Skip if no
            // node found (defensive — every tool_result should land on a
            // tool_call node post-merge).
            let node_id = match find_node_for_event(graph_nodes, &ev.event_id) {
                Some(id) => id,
                None => continue,
            };
            let finding_id = derive_node_id(
                "finding",
                &[
                    ("session_id", session_id),
                    ("category", CATEGORY),
                    ("evidence_event_id", ev.event_id.as_str()),
                ],
            );
            // Rewrite the `nd_` prefix to `find_` for clarity at read-time.
            let finding_id = finding_id.replacen("nd_", "find_", 1);
            out.push(NewFinding {
                finding_id,
                schema_version: SCHEMA_VERSION.into(),
                session_id: session_id.into(),
                category: CATEGORY.into(),
                severity: SEVERITY.into(),
                claim: CLAIM.into(),
                confidence: CONFIDENCE,
                limitations: json!([
                    "User-rejection vs tool-error not distinguished in this rule version."
                ]),
                evidence_refs: json!([
                    { "node_id": node_id, "role": "supporting" }
                ]),
                generated_at: generated_at.into(),
                rule_version: RULE_VERSION.into(),
            });
        }
        out
    }
}

fn find_node_for_event<'a>(nodes: &'a [GraphNode], event_id: &str) -> Option<&'a str> {
    nodes
        .iter()
        .find(|n| n.source_event_ids.iter().any(|s| s == event_id))
        .map(|n| n.node_id.as_str())
}
