//! loop-foundations 2026-06-12 — MCP 도구: whats_in_my_cc.get_project_metrics
//!
//! `GET /v1/metrics`와 동일한 세션 횡단 metrics+fingerprint series.
//! session-retrospect 스킬의 전후 비교(개입 효과 귀속)가 단일 툴콜로 끝나게
//! 한다. count와 관측 값만 — 판단은 호출자(LLM) 몫.

use serde_json::{json, Value};
use sqlx::SqlitePool;

use crate::api::mcp::tools::{tool_error, tool_success};
use crate::insight::series;

pub async fn call(args: &Value, pool: &SqlitePool) -> Value {
    let project = args["project"]
        .as_str()
        .map(|p| p.trim_end_matches('/').to_string())
        .filter(|p| !p.is_empty());
    let parse_time = |key: &str| -> Result<Option<chrono::DateTime<chrono::Utc>>, String> {
        match args[key].as_str() {
            None => Ok(None),
            Some(v) => chrono::DateTime::parse_from_rfc3339(v)
                .map(|d| Some(d.with_timezone(&chrono::Utc)))
                .map_err(|_| format!("{key} must be RFC3339")),
        }
    };
    let from = match parse_time("from") {
        Ok(v) => v,
        Err(e) => return tool_error(e),
    };
    let to = match parse_time("to") {
        Ok(v) => v,
        Err(e) => return tool_error(e),
    };
    let limit = args["limit"].as_i64().unwrap_or(series::DEFAULT_LIMIT);
    match series::collect(pool, project.as_deref(), from, to, limit).await {
        Ok(s) => match serde_json::to_value(&s) {
            Ok(data) => tool_success(json!({ "data": data })),
            Err(e) => tool_error(format!("serialize error: {e}")),
        },
        Err(e) => tool_error(format!("db error: {e}")),
    }
}
