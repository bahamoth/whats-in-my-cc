//! Slice-17 — MCP tool: whats_in_my_cc.search_sessions

use serde_json::{json, Value};
use sqlx::SqlitePool;

use crate::api::mcp::tools::{tool_error, tool_success};
use crate::db::repo_observed;

pub async fn call(args: &Value, pool: &SqlitePool) -> Value {
    let limit = args["limit"].as_i64().unwrap_or(20).clamp(1, 200);
    // Dogfood 2026-06-12 (§3-3) — same project filter as
    // `GET /v1/sessions?project=`: the session-retrospect skill resolves
    // "this project's sessions" from the project root it runs in.
    let project = args["project"]
        .as_str()
        .map(|p| p.trim_end_matches('/'))
        .filter(|p| !p.is_empty());

    let rows = match repo_observed::list_sessions_filtered(pool, limit, project).await {
        Ok(r) => r,
        Err(e) => return tool_error(format!("db error: {e}")),
    };

    let data: Vec<Value> = rows
        .into_iter()
        .map(|r| {
            json!({
                "session_id": r.session_id,
                "first_observed_at": r.first_observed_at,
                "last_observed_at": r.last_observed_at,
                "event_count": r.event_count
            })
        })
        .collect();

    tool_success(json!({ "data": data }))
}
