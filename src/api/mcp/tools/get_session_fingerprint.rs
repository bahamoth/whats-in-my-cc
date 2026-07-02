//! MCP parity (2026-07-03) — MCP tool: whats_in_my_cc.get_session_fingerprint
//!
//! `GET /v1/sessions/:id/fingerprint`와 동일한 세션 환경 fingerprint.
//! 자기개선 루프의 독립변수 표면 — 관측 값만, 판단 필드 없음.

use serde_json::{json, Value};
use sqlx::SqlitePool;

use crate::api::mcp::tools::{tool_error, tool_success};
use crate::insight::fingerprint;

pub async fn call(args: &Value, pool: &SqlitePool) -> Value {
    let Some(session_id) = args["session_id"].as_str().filter(|s| !s.is_empty()) else {
        return tool_error("session_id is required");
    };
    match fingerprint::compute_session_fingerprint(pool, session_id).await {
        Ok(f) => match serde_json::to_value(&f) {
            Ok(data) => tool_success(json!({ "data": data })),
            Err(e) => tool_error(format!("serialize error: {e}")),
        },
        Err(e) => tool_error(format!("db error: {e}")),
    }
}
