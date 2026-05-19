use sqlx::{Row, SqlitePool};

use crate::error::Result;
use crate::model::observed::{Actor, EventKind, ObservedEvent, TelemetryFacet};

pub async fn insert(pool: &SqlitePool, e: &ObservedEvent) -> Result<()> {
    sqlx::query(
        "INSERT INTO observed_event(
            event_id, raw_event_id, schema_version, session_id, event_uuid, parent_uuid,
            observed_at, actor, kind, subkind, tool_use_id, tool_name, request_id,
            message_id, turn_id, source_tool_assistant_uuid, source_tool_use_id,
            is_sidechain, is_meta, cwd, git_branch, user_type, entrypoint, cc_version,
            trace_id, span_id, parent_span_id, latency_ms,
            payload, parser_version)
         VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind(&e.event_id)
    .bind(&e.raw_event_id)
    .bind(&e.schema_version)
    .bind(&e.session_id)
    .bind(&e.event_uuid)
    .bind(&e.parent_uuid)
    .bind(e.observed_at.to_rfc3339())
    .bind(e.actor.as_str())
    .bind(e.kind.as_str())
    .bind(&e.subkind)
    .bind(&e.tool_use_id)
    .bind(&e.tool_name)
    .bind(&e.request_id)
    .bind(&e.message_id)
    .bind(&e.turn_id)
    .bind(&e.source_tool_assistant_uuid)
    .bind(&e.source_tool_use_id)
    .bind(e.is_sidechain as i64)
    .bind(e.is_meta as i64)
    .bind(&e.cwd)
    .bind(&e.git_branch)
    .bind(&e.user_type)
    .bind(&e.entrypoint)
    .bind(&e.cc_version)
    .bind(&e.trace_id)
    .bind(&e.span_id)
    .bind(&e.parent_span_id)
    .bind(e.latency_ms)
    .bind(merge_payload_with_telemetry(&e.payload, e.telemetry.as_ref()).to_string())
    .bind(&e.parser_version)
    .execute(pool)
    .await?;
    Ok(())
}

fn merge_payload_with_telemetry(
    payload: &serde_json::Value,
    telemetry: Option<&TelemetryFacet>,
) -> serde_json::Value {
    let mut out = if payload.is_object() {
        payload.clone()
    } else {
        serde_json::json!({ "value": payload })
    };
    if let Some(t) = telemetry {
        if let serde_json::Value::Object(map) = &mut out {
            map.insert(
                "telemetry".into(),
                serde_json::to_value(t).unwrap_or(serde_json::Value::Null),
            );
        }
    }
    out
}

pub struct SessionRow {
    pub session_id: String,
    pub first_observed_at: String,
    pub last_observed_at: String,
    pub event_count: i64,
}

pub async fn list_sessions(pool: &SqlitePool, limit: i64) -> Result<Vec<SessionRow>> {
    let rows = sqlx::query(
        "SELECT session_id,
                MIN(observed_at) AS first_observed_at,
                MAX(observed_at) AS last_observed_at,
                COUNT(*)         AS event_count
         FROM observed_event GROUP BY session_id ORDER BY last_observed_at DESC LIMIT ?",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| SessionRow {
            session_id: r.get("session_id"),
            first_observed_at: r.get("first_observed_at"),
            last_observed_at: r.get("last_observed_at"),
            event_count: r.get("event_count"),
        })
        .collect())
}

pub async fn list_session(
    pool: &SqlitePool,
    session_id: &str,
    limit: i64,
) -> Result<Vec<ObservedEvent>> {
    let rows = sqlx::query(
        "SELECT * FROM observed_event WHERE session_id = ? ORDER BY observed_at ASC LIMIT ?",
    )
    .bind(session_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(row_to_observed).collect())
}

fn row_to_observed(r: sqlx::sqlite::SqliteRow) -> ObservedEvent {
    let actor: String = r.get("actor");
    let kind: String = r.get("kind");
    let payload_str: String = r.get("payload");
    let mut payload: serde_json::Value =
        serde_json::from_str(&payload_str).unwrap_or(serde_json::Value::Null);
    let telemetry = if let serde_json::Value::Object(map) = &mut payload {
        map.remove("telemetry")
            .and_then(|v| serde_json::from_value(v).ok())
    } else {
        None
    };
    ObservedEvent {
        event_id: r.get("event_id"),
        raw_event_id: r.get("raw_event_id"),
        schema_version: r.get("schema_version"),
        parser_version: r.get("parser_version"),
        session_id: r.get("session_id"),
        event_uuid: r.try_get("event_uuid").ok(),
        parent_uuid: r.try_get("parent_uuid").ok(),
        observed_at: chrono::DateTime::parse_from_rfc3339(&r.get::<String, _>("observed_at"))
            .unwrap()
            .with_timezone(&chrono::Utc),
        actor: match actor.as_str() {
            "user" => Actor::User,
            "assistant" => Actor::Assistant,
            "hook" => Actor::Hook,
            "tool" => Actor::Tool,
            _ => Actor::System,
        },
        kind: match kind.as_str() {
            "user_message" => EventKind::UserMessage,
            "assistant_message" => EventKind::AssistantMessage,
            "thinking" => EventKind::Thinking,
            "tool_call" => EventKind::ToolCall,
            "tool_result" => EventKind::ToolResult,
            "hook_event" => EventKind::HookEvent,
            "system_summary" => EventKind::SystemSummary,
            "session_state" => EventKind::SessionState,
            "file_history_snapshot" => EventKind::FileHistorySnapshot,
            "attachment_meta" => EventKind::AttachmentMeta,
            "otel_span" => EventKind::OtelSpan,
            _ => EventKind::Unknown,
        },
        subkind: r.try_get("subkind").ok(),
        tool_use_id: r.try_get("tool_use_id").ok(),
        tool_name: r.try_get("tool_name").ok(),
        request_id: r.try_get("request_id").ok(),
        message_id: r.try_get("message_id").ok(),
        turn_id: r.try_get("turn_id").ok(),
        source_tool_assistant_uuid: r.try_get("source_tool_assistant_uuid").ok(),
        source_tool_use_id: r.try_get("source_tool_use_id").ok(),
        is_sidechain: r.get::<i64, _>("is_sidechain") != 0,
        is_meta: r.get::<i64, _>("is_meta") != 0,
        cwd: r.try_get("cwd").ok(),
        git_branch: r.try_get("git_branch").ok(),
        user_type: r.try_get("user_type").ok(),
        entrypoint: r.try_get("entrypoint").ok(),
        cc_version: r.try_get("cc_version").ok(),
        trace_id: r.try_get("trace_id").ok(),
        span_id: r.try_get("span_id").ok(),
        parent_span_id: r.try_get("parent_span_id").ok(),
        latency_ms: r.try_get("latency_ms").ok(),
        telemetry,
        payload,
    }
}
