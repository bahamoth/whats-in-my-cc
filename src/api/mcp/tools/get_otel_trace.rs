//! Slice-17 — MCP tool: whats_in_my_cc.get_otel_trace

use serde_json::{json, Value};
use sqlx::Row;
use sqlx::SqlitePool;

use crate::api::mcp::tools::{tool_error, tool_success};

pub async fn call(args: &Value, pool: &SqlitePool) -> Value {
    let trace_id = match args["trace_id"].as_str() {
        Some(s) => s,
        None => return tool_error("missing required argument: trace_id"),
    };

    // Query observed_event for OTel spans matching this trace_id.
    let rows = sqlx::query(
        "SELECT event_id, session_id, observed_at, span_id, parent_span_id, payload \
         FROM observed_event \
         WHERE kind = 'otel_span' AND trace_id = ? \
         ORDER BY observed_at ASC \
         LIMIT 200",
    )
    .bind(trace_id)
    .fetch_all(pool)
    .await;

    let rows = match rows {
        Ok(r) => r,
        Err(e) => return tool_error(format!("db error: {e}")),
    };

    let spans: Vec<Value> = rows
        .into_iter()
        .map(|r| {
            let event_id: String = r.get("event_id");
            let session_id: String = r.get("session_id");
            let observed_at: String = r.get("observed_at");
            let span_id: Option<String> = r.try_get("span_id").ok().flatten();
            let parent_span_id: Option<String> = r.try_get("parent_span_id").ok().flatten();
            let payload_str: Option<String> = r.try_get("payload").ok().flatten();
            let payload: Value = payload_str
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or(Value::Null);
            json!({
                "event_id": event_id,
                "session_id": session_id,
                "observed_at": observed_at,
                "span_id": span_id,
                "parent_span_id": parent_span_id,
                "payload": payload
            })
        })
        .collect();

    tool_success(json!({
        "trace_id": trace_id,
        "spans": spans
    }))
}
