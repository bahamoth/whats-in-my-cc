//! Dogfood 2026-06-12 (§3-2) — MCP tool: whats_in_my_cc.get_session_turns
//!
//! Same rollup as `GET /v1/sessions/:id/turns`; the session-retrospect skill
//! consumes this over MCP so a retrospect needs no custom scripting.

use serde_json::{json, Value};
use sqlx::SqlitePool;

use crate::api::mcp::tools::{tool_error, tool_success};
use crate::db::repo_observed;
use crate::insight::turn_rollup;

/// B-3 (2026-07-04): 대형 세션에서 전체 rollup이 토큰 비효율적 —
/// limit/offset 페이지네이션을 더하고 절단은 total_count로 노출한다
/// (silent cap 금지 — get_otel_trace matched_count 선례).
const DEFAULT_LIMIT: usize = 100;
const MAX_LIMIT: usize = 500;

pub async fn call(args: &Value, pool: &SqlitePool) -> Value {
    let Some(session_id) = args["session_id"].as_str().filter(|s| !s.is_empty()) else {
        return tool_error("session_id is required");
    };
    let limit = args["limit"]
        .as_u64()
        .map(|v| v as usize)
        .unwrap_or(DEFAULT_LIMIT)
        .clamp(1, MAX_LIMIT);
    let offset = args["offset"].as_u64().map(|v| v as usize).unwrap_or(0);
    let evs = match repo_observed::list_session_conversation(pool, session_id).await {
        Ok(e) => e,
        Err(e) => return tool_error(format!("db error: {e}")),
    };
    let mut rollup = turn_rollup::rollup(session_id, &evs);
    let total_count = rollup.turns.len();
    rollup.turns = rollup.turns.into_iter().skip(offset).take(limit).collect();
    match serde_json::to_value(&rollup) {
        Ok(mut data) => {
            if let Some(obj) = data.as_object_mut() {
                obj.insert("total_count".into(), json!(total_count));
                obj.insert("offset".into(), json!(offset));
            }
            tool_success(json!({ "data": data }))
        }
        Err(e) => tool_error(format!("serialize error: {e}")),
    }
}
