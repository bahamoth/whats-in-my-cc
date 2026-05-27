//! Slice-17 — MCP tool: whats_in_my_cc.explain_node
//!
//! Combines graph node details + any findings that reference the node's events.

use serde_json::{json, Value};
use sqlx::SqlitePool;

use crate::api::mcp::tools::{tool_error, tool_success};
use crate::db::{repo_finding, repo_graph};

pub async fn call(args: &Value, pool: &SqlitePool) -> Value {
    let session_id = match args["session_id"].as_str() {
        Some(s) => s,
        None => return tool_error("missing required argument: session_id"),
    };
    let node_id = match args["node_id"].as_str() {
        Some(s) => s,
        None => return tool_error("missing required argument: node_id"),
    };

    // Load all graph nodes for the session and find the target.
    let (nodes, edges) = match repo_graph::load_session(pool, session_id).await {
        Ok(v) => v,
        Err(e) => return tool_error(format!("db error: {e}")),
    };

    let node = nodes.iter().find(|n| n.node_id == node_id);
    let node_value = match node {
        Some(n) => serde_json::to_value(n).unwrap_or(Value::Null),
        None => {
            return tool_success(json!({
                "found": false,
                "node_id": node_id,
                "message": "node not found in session graph"
            }));
        }
    };

    // Edges connected to this node.
    let connected_edges: Vec<Value> = edges
        .iter()
        .filter(|e| e.from_node_id == node_id || e.to_node_id == node_id)
        .map(|e| serde_json::to_value(e).unwrap_or(Value::Null))
        .collect();

    // Findings that reference any of the node's source_event_ids.
    let source_event_ids = node
        .map(|n| n.source_event_ids.as_slice())
        .unwrap_or_default();

    let all_findings = repo_finding::list(
        pool,
        &repo_finding::ListFilter {
            session_id: Some(session_id.to_string()),
            limit: 200,
            ..Default::default()
        },
    )
    .await
    .unwrap_or_default();

    let related_findings: Vec<Value> = all_findings
        .into_iter()
        .filter(|f| {
            let ev_refs: Vec<String> = serde_json::from_str(&f.evidence_refs).unwrap_or_default();
            source_event_ids.iter().any(|id| ev_refs.contains(id))
        })
        .map(|f| json!({
            "finding_id": f.finding_id,
            "category": f.category,
            "severity": f.severity,
            "summary": f.summary
        }))
        .collect();

    tool_success(json!({
        "found": true,
        "node": node_value,
        "connected_edges": connected_edges,
        "related_findings": related_findings
    }))
}
