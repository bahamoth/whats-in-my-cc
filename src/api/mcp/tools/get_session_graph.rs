//! Slice-17 — MCP tool: whats_in_my_cc.get_session_graph

use serde_json::{json, Value};
use sqlx::SqlitePool;

use crate::api::mcp::tools::{tool_error, tool_success};
use crate::db::repo_graph;

pub async fn call(args: &Value, pool: &SqlitePool) -> Value {
    let session_id = match args["session_id"].as_str() {
        Some(s) => s,
        None => return tool_error("missing required argument: session_id"),
    };

    let (nodes, edges) = match repo_graph::load_session(pool, session_id).await {
        Ok(v) => v,
        Err(e) => return tool_error(format!("db error: {e}")),
    };

    let envelope = json!({
        "meta": { "schema_version": crate::model::meta::SCHEMA_VERSION, "generated_at": chrono::Utc::now().to_rfc3339() },
        "data": {
            "nodes": nodes.iter().map(|n| serde_json::to_value(n).unwrap_or(Value::Null)).collect::<Vec<_>>(),
            "edges": edges.iter().map(|e| serde_json::to_value(e).unwrap_or(Value::Null)).collect::<Vec<_>>()
        }
    });

    tool_success(envelope)
}
