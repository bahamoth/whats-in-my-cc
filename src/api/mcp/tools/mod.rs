//! Slice-17 — MCP tool catalogue.
//!
//! Read-only tools. Each delegates to the existing Pull API data layer.
//! Tool outputs are wrapped in a single `text` content block (DEV-S17-03).
//!
//! Plan 1: the `search_findings` tool was removed with the finding subsystem.
//! A signal tool is deferred to a later plan (Plan 4).

use serde_json::{json, Value};
use sqlx::SqlitePool;

pub mod get_file_lineage;
pub mod get_otel_trace;
pub mod get_session_turns;
pub mod list_detectors;
pub mod search_sessions;

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
fn search_sessions_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "limit": { "type": "integer", "default": 20, "description": "Max sessions to return" },
            "project": { "type": "string", "description": "Project root path — only sessions with ≥1 event whose cwd equals this path (trailing slash ignored)" }
        },
        "required": []
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

fn get_session_turns_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "session_id": { "type": "string", "description": "Session ID" }
        },
        "required": ["session_id"]
    })
}

fn list_detectors_schema() -> Value {
    json!({
        "type": "object",
        "properties": {},
        "required": []
    })
}

/// Build the tools/list response body.
pub fn tools_list_response() -> Value {
    json!({
        "tools": [
            {
                "name": "whats_in_my_cc.search_sessions",
                "description": "List recent Claude Code sessions observed locally.",
                "inputSchema": search_sessions_schema()
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
            },
            {
                "name": "whats_in_my_cc.get_session_turns",
                "description": "Turn-level deterministic rollup of a session: per-user-turn tool histogram, edited files, and cross-turn file churn. Counts and redacted excerpts only — judgments (e.g. rework classification) are the caller's.",
                "inputSchema": get_session_turns_schema()
            },
            {
                "name": "whats_in_my_cc.list_detectors",
                "description": "Return the manifest catalog for all registered detectors (spec §6.4). Each manifest describes what the detector detects, which raw payload fields it reads, by what rule, and why. Use this before proposing config changes or new detectors.",
                "inputSchema": list_detectors_schema()
            }
        ]
    })
}

/// Dispatch a tools/call request to the appropriate handler.
pub async fn dispatch(name: &str, args: &Value, pool: &SqlitePool) -> Value {
    match name {
        "whats_in_my_cc.search_sessions" => search_sessions::call(args, pool).await,
        "whats_in_my_cc.get_file_lineage" => get_file_lineage::call(args, pool).await,
        "whats_in_my_cc.get_otel_trace" => get_otel_trace::call(args, pool).await,
        "whats_in_my_cc.get_session_turns" => get_session_turns::call(args, pool).await,
        "whats_in_my_cc.list_detectors" => list_detectors::call(args, pool).await,
        _ => tool_error(format!("unknown tool: {name}")),
    }
}
