//! MCP parity (2026-07-03) — MCP tool: whats_in_my_cc.get_session_metrics
//!
//! `GET /v1/sessions/:id/metrics`와 동일한 on-demand SessionMetrics. 회고 흐름의
//! 세션 단위 리소스가 HTTP 전용이라 순수 MCP 클라이언트(session-retrospect 스킬
//! 포함)가 폴백 없이 완주하지 못하던 격차를 닫는다. count만 — 판단은 호출자 몫.

use serde_json::{json, Value};
use sqlx::SqlitePool;

use crate::api::mcp::tools::{tool_error, tool_success};
use crate::insight::metrics;

pub async fn call(args: &Value, pool: &SqlitePool) -> Value {
    let Some(session_id) = args["session_id"].as_str().filter(|s| !s.is_empty()) else {
        return tool_error("session_id is required");
    };
    match metrics::compute_session_metrics(pool, session_id).await {
        Ok(m) => match serde_json::to_value(&m) {
            Ok(data) => tool_success(json!({ "data": data })),
            Err(e) => tool_error(format!("serialize error: {e}")),
        },
        Err(e) => tool_error(format!("db error: {e}")),
    }
}
