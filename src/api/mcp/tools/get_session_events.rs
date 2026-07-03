//! B-3 (2026-07-04) — MCP tool: whats_in_my_cc.get_session_events
//!
//! 순수 MCP 클라이언트의 원문 이벤트 창 접근 갭을 닫는다(종전에는 raw
//! 이벤트가 HTTP 전용). `GET /v1/sessions/:id/events`와 같은 커서 계약:
//! `prev_cursor`/`next_cursor`는 `<rfc3339>|<event_id>`, next=null이면 이미
//! 라이브 tip이다. HTTP 쪽의 kind 필터·around·correlation 조회는 UI 전용
//! 기능이라 싣지 않는다 — 창 계약만 1:1.

use serde_json::{json, Value};
use sqlx::SqlitePool;

use crate::api::mcp::tools::{tool_error, tool_success};
use crate::api::routes::observed_to_dto;
use crate::db::repo_observed;
use crate::model::cursor::Cursor;

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 500;

pub async fn call(args: &Value, pool: &SqlitePool) -> Value {
    let Some(session_id) = args["session_id"].as_str().filter(|s| !s.is_empty()) else {
        return tool_error("session_id is required");
    };
    let limit = args["limit"]
        .as_i64()
        .unwrap_or(DEFAULT_LIMIT)
        .clamp(1, MAX_LIMIT);
    let parse_cursor = |key: &str| -> Result<Option<Cursor>, String> {
        match args[key].as_str() {
            None => Ok(None),
            Some(s) => s
                .parse::<Cursor>()
                .map(Some)
                .map_err(|e| format!("invalid {key} cursor: {e}")),
        }
    };
    let before = match parse_cursor("before") {
        Ok(c) => c,
        Err(e) => return tool_error(e),
    };
    let after = match parse_cursor("after") {
        Ok(c) => c,
        Err(e) => return tool_error(e),
    };

    let evs = match repo_observed::list_session_window(
        pool,
        session_id,
        before.as_ref(),
        after.as_ref(),
        limit,
    )
    .await
    {
        Ok(e) => e,
        Err(e) => return tool_error(format!("db error: {e}")),
    };

    let (prev_cursor, next_cursor) = match (evs.first(), evs.last()) {
        (Some(first), Some(last)) => {
            let prev = format!("{}|{}", first.observed_at.to_rfc3339(), first.event_id);
            // HTTP 핸들러와 같은 tip 규칙: 세션의 last_observed_at에 닿았으면
            // next=null (이후는 SSE/재조회 몫).
            let summary = match repo_observed::session_summary(pool, session_id).await {
                Ok(s) => s,
                Err(e) => return tool_error(format!("db error: {e}")),
            };
            let at_tip =
                matches!(summary, Some((_, _, ref tip)) if *tip == last.observed_at.to_rfc3339());
            let next = if at_tip {
                None
            } else {
                Some(format!(
                    "{}|{}",
                    last.observed_at.to_rfc3339(),
                    last.event_id
                ))
            };
            (Some(prev), next)
        }
        _ => (None, None),
    };

    let events: Vec<Value> = evs.iter().map(observed_to_dto).collect();
    tool_success(json!({ "data": {
        "events": events,
        "prev_cursor": prev_cursor,
        "next_cursor": next_cursor,
    }}))
}
