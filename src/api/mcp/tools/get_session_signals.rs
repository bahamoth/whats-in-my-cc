//! MCP parity (2026-07-03) — MCP tool: whats_in_my_cc.get_session_signals
//!
//! `GET /v1/sessions/:id/signals`와 동일한 evidence-linked L1 Signal 목록.
//! DTO 변환은 HTTP 핸들러와 같은 `signal_row_to_dto`를 공유한다 — 두 표면이
//! 같은 형태를 반환해야 소비자가 전송 계층에 무관하게 동작한다.

use serde_json::{json, Value};
use sqlx::SqlitePool;

use crate::api::mcp::tools::{tool_error, tool_success};
use crate::api::routes::signal_row_to_dto;
use crate::db::repo_signal;

pub async fn call(args: &Value, pool: &SqlitePool) -> Value {
    let Some(session_id) = args["session_id"].as_str().filter(|s| !s.is_empty()) else {
        return tool_error("session_id is required");
    };
    match repo_signal::list_by_session(pool, session_id).await {
        Ok(rows) => {
            let dtos: Vec<_> = rows.into_iter().map(signal_row_to_dto).collect();
            match serde_json::to_value(&dtos) {
                Ok(data) => tool_success(json!({ "data": data })),
                Err(e) => tool_error(format!("serialize error: {e}")),
            }
        }
        Err(e) => tool_error(format!("db error: {e}")),
    }
}
