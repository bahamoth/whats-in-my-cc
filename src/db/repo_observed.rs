use sqlx::{Row, SqlitePool};

use crate::error::Result;
use crate::model::observed::{Actor, EventKind, ObservedEvent, TelemetryFacet};

pub async fn insert(pool: &SqlitePool, e: &ObservedEvent) -> Result<()> {
    insert_inner(pool, e, false).await.map(|_| ())
}

/// slice-6 — insert that skips on PK conflict. Returns true when a new row was
/// added, false when `event_id` already existed. Used by reparse-friendly
/// ingesters (otel metrics / logs) where the same data point may be normalised
/// more than once across Stage 1 → Stage 2 transitions.
pub async fn insert_or_ignore(pool: &SqlitePool, e: &ObservedEvent) -> Result<bool> {
    insert_inner(pool, e, true).await
}

async fn insert_inner(pool: &SqlitePool, e: &ObservedEvent, ignore: bool) -> Result<bool> {
    let sql = if ignore {
        "INSERT OR IGNORE INTO observed_event(
            event_id, raw_event_id, schema_version, session_id, event_uuid, parent_uuid,
            observed_at, actor, kind, subkind, tool_use_id, tool_name, request_id,
            message_id, turn_id, source_tool_assistant_uuid, source_tool_use_id,
            is_sidechain, is_meta, cwd, git_branch, user_type, entrypoint, cc_version,
            trace_id, span_id, parent_span_id, latency_ms,
            payload, parser_version)
         VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)"
    } else {
        "INSERT INTO observed_event(
            event_id, raw_event_id, schema_version, session_id, event_uuid, parent_uuid,
            observed_at, actor, kind, subkind, tool_use_id, tool_name, request_id,
            message_id, turn_id, source_tool_assistant_uuid, source_tool_use_id,
            is_sidechain, is_meta, cwd, git_branch, user_type, entrypoint, cc_version,
            trace_id, span_id, parent_span_id, latency_ms,
            payload, parser_version)
         VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)"
    };
    let res = sqlx::query(sql)
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
    Ok(res.rows_affected() > 0)
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
    /// slice-7 — per-source row counts so the WebUI can show transcript-only
    /// vs OTel-only sessions at a glance without a second round trip.
    pub by_kind: std::collections::BTreeMap<String, i64>,
}

pub async fn list_sessions(pool: &SqlitePool, limit: i64) -> Result<Vec<SessionRow>> {
    use sqlx::Row as _Row;
    // First pass: per-session totals + ordering. Limit applies here.
    let totals = sqlx::query(
        "SELECT session_id,
                MIN(observed_at) AS first_observed_at,
                MAX(observed_at) AS last_observed_at,
                COUNT(*)         AS event_count
         FROM observed_event WHERE session_id != ''
         GROUP BY session_id ORDER BY last_observed_at DESC LIMIT ?",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    if totals.is_empty() {
        return Ok(Vec::new());
    }
    // Second pass: by_kind for just the session_ids we returned. Inlining the
    // IN(?) list with a dynamic placeholder list keeps it sqlx-friendly.
    let ids: Vec<String> = totals.iter().map(|r| r.get::<String, _>("session_id")).collect();
    let placeholders = (0..ids.len()).map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT session_id, kind, COUNT(*) AS n
           FROM observed_event
          WHERE session_id IN ({placeholders})
          GROUP BY session_id, kind"
    );
    let mut q = sqlx::query(&sql);
    for id in &ids {
        q = q.bind(id);
    }
    let kind_rows = q.fetch_all(pool).await?;
    let mut by_kind_map: std::collections::HashMap<String, std::collections::BTreeMap<String, i64>> =
        std::collections::HashMap::new();
    for r in kind_rows {
        let sid: String = r.get("session_id");
        let kind: String = r.get("kind");
        let n: i64 = r.get("n");
        by_kind_map.entry(sid).or_default().insert(kind, n);
    }
    Ok(totals
        .into_iter()
        .map(|r| {
            let sid: String = r.get("session_id");
            let by_kind = by_kind_map.remove(&sid).unwrap_or_default();
            SessionRow {
                session_id: sid,
                first_observed_at: r.get("first_observed_at"),
                last_observed_at: r.get("last_observed_at"),
                event_count: r.get("event_count"),
                by_kind,
            }
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

/// Slice-8 — newest `limit` events of a session, ordered DESC so we don't have
/// to scan from the start. Used by the WebUI live timeline: sessions with more
/// than `limit` rows would otherwise show only the oldest window with the
/// ASC variant above, and the live tail's newest envelopes would never appear
/// in the rendered page. The handler reverses before serialising so the wire
/// order remains ASC (consumers expect a chronological timeline).
///
/// Long-term, this is replaced by windowed range queries (slice-9 follow-up):
/// `?from=&to=&limit=` with a client-side LRU chunk cache + virtualisation,
/// matching the video-streaming buffer pattern.
pub async fn list_session_latest(
    pool: &SqlitePool,
    session_id: &str,
    limit: i64,
) -> Result<Vec<ObservedEvent>> {
    let rows = sqlx::query(
        "SELECT * FROM observed_event WHERE session_id = ? ORDER BY observed_at DESC LIMIT ?",
    )
    .bind(session_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(row_to_observed).collect())
}

/// Slice-8 — accurate per-session summary (count + first/last observed_at)
/// independent of the `list_session_latest` window. Without this the WebUI
/// MetaStrip would show the first/last of the 5000-event window instead of
/// the true session boundaries.
pub async fn session_summary(
    pool: &SqlitePool,
    session_id: &str,
) -> Result<Option<(i64, String, String)>> {
    let row: Option<(i64, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT COUNT(*), MIN(observed_at), MAX(observed_at) \
         FROM observed_event WHERE session_id = ?",
    )
    .bind(session_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.and_then(|(c, f, l)| match (f, l) {
        (Some(f), Some(l)) if c > 0 => Some((c, f, l)),
        _ => None,
    }))
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
            "file_event" => EventKind::FileEvent,
            "git_commit" => EventKind::GitCommit,
            "diff_hunk" => EventKind::DiffHunk,
            "metric_sample" => EventKind::MetricSample,
            "log_record" => EventKind::LogRecord,
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
