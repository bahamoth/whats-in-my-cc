//! Dogfood 2026-06-12 (§3-2) — MCP tool: whats_in_my_cc.get_session_turns
//!
//! Same rollup as `GET /v1/sessions/:id/turns`; the session-retrospect skill
//! consumes this over MCP so a retrospect needs no custom scripting.

use serde_json::{json, Value};
use sqlx::SqlitePool;

use crate::api::mcp::tools::{tool_error, tool_success};
use crate::db::repo_observed;
use crate::insight::turn_rollup;

pub async fn call(args: &Value, pool: &SqlitePool) -> Value {
    let Some(session_id) = args["session_id"].as_str().filter(|s| !s.is_empty()) else {
        return tool_error("session_id is required");
    };
    let evs = match repo_observed::list_session_conversation(pool, session_id).await {
        Ok(e) => e,
        Err(e) => return tool_error(format!("db error: {e}")),
    };
    let rollup = turn_rollup::rollup(session_id, &evs);
    match serde_json::to_value(&rollup) {
        Ok(data) => tool_success(json!({ "data": data })),
        Err(e) => tool_error(format!("serialize error: {e}")),
    }
}
