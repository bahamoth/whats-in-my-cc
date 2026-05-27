//! Slice-17 — MCP tool catalogue.
//!
//! Six read-only tools. Each delegates to the existing Pull API data layer.
//! Tool outputs are wrapped in a single `text` content block (DEV-S17-03).

use serde_json::{json, Value};
use sqlx::SqlitePool;

pub mod get_file_lineage;
pub mod get_otel_trace;
pub mod get_session_graph;
pub mod search_findings;
pub mod search_sessions;
pub mod explain_node;

/// Wrap a JSON value as an MCP `tools/call` success result.
pub fn tool_success(data: Value) -> Value {
    let text = serde_json::to_string(&data).unwrap_or_else(|_| "{}".into());
    json!({
        "content": [{ "type": "text", "text": text }],
        "isError": false
    })
}

/// Wrap an error string as an MCP `tools/call` error result.
pub fn tool_error(msg: impl Into<String>) -> Value {
    json!({
        "content": [{ "type": "text", "text": msg.into() }],
        "isError": true
    })
}

/// Canonical tool input schema definitions (for tools/list).
fn get_session_graph_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "session_id": { "type": "string", "description": "Session ID" },
            "include_sources": { "type": "boolean", "default": true, "description": "Include source event IDs on nodes" }
        },
        "required": ["session_id"]
    })
}

fn search_sessions_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "limit": { "type": "integer", "default": 20, "description": "Max sessions to return" }
        },
        "required": []
    })
}

fn search_findings_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "session_id": { "type": "string", "description": "Filter by session ID" },
            "category": { "type": "string", "description": "Filter by finding category" },
            "severity": { "type": "string", "description": "Filter by severity" },
            "limit": { "type": "integer", "default": 50 }
        },
        "required": []
    })
}

fn explain_node_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "session_id": { "type": "string", "description": "Session ID" },
            "node_id": { "type": "string", "description": "Graph node ID" }
        },
        "required": ["session_id", "node_id"]
    })
}

fn get_file_lineage_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "session_id": { "type": "string", "description": "Session ID" },
            "file_path": { "type": "string", "description": "File path to trace lineage for" }
        },
        "required": ["session_id", "file_path"]
    })
}

fn get_otel_trace_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "trace_id": { "type": "string", "description": "OTel trace ID (hex)" }
        },
        "required": ["trace_id"]
    })
}

/// Build the tools/list response body.
pub fn tools_list_response() -> Value {
    json!({
        "tools": [
            {
                "name": "whats_in_my_cc.get_session_graph",
                "description": "Return the execution graph (nodes + edges) for a Claude Code session.",
                "inputSchema": get_session_graph_schema()
            },
            {
                "name": "whats_in_my_cc.search_sessions",
                "description": "List recent Claude Code sessions observed locally.",
                "inputSchema": search_sessions_schema()
            },
            {
                "name": "whats_in_my_cc.search_findings",
                "description": "Search for insight findings (tool failures, missing verification, risky actions, etc.).",
                "inputSchema": search_findings_schema()
            },
            {
                "name": "whats_in_my_cc.explain_node",
                "description": "Explain a specific graph node: what it is, what findings it relates to.",
                "inputSchema": explain_node_schema()
            },
            {
                "name": "whats_in_my_cc.get_file_lineage",
                "description": "Return the diff-hunk lineage for a specific file in a session.",
                "inputSchema": get_file_lineage_schema()
            },
            {
                "name": "whats_in_my_cc.get_otel_trace",
                "description": "Return OTel spans for a trace ID observed in this session.",
                "inputSchema": get_otel_trace_schema()
            }
        ]
    })
}

/// Dispatch a tools/call request to the appropriate handler.
pub async fn dispatch(name: &str, args: &Value, pool: &SqlitePool) -> Value {
    match name {
        "whats_in_my_cc.get_session_graph" => {
            get_session_graph::call(args, pool).await
        }
        "whats_in_my_cc.search_sessions" => {
            search_sessions::call(args, pool).await
        }
        "whats_in_my_cc.search_findings" => {
            search_findings::call(args, pool).await
        }
        "whats_in_my_cc.explain_node" => {
            explain_node::call(args, pool).await
        }
        "whats_in_my_cc.get_file_lineage" => {
            get_file_lineage::call(args, pool).await
        }
        "whats_in_my_cc.get_otel_trace" => {
            get_otel_trace::call(args, pool).await
        }
        _ => tool_error(format!("unknown tool: {name}")),
    }
}
