use sqlx::{Row, SqlitePool};

use crate::error::Result;
use crate::model::cursor::Cursor;
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

/// Slice-9 — windowed range query for paged event reads. Supersedes
/// `list_session_latest` in handler usage; `list_session_latest` remains as
/// the no-cursor convenience for SSE backfill's `last_event_id` path.
///
/// Ordering: `(observed_at, event_id)` ASC on the wire (DEV-S8-10 lock).
/// SQL: when `before` is set or no cursors → DESC LIMIT then reverse in
/// memory so we always return the most relevant window without a full scan;
/// when only `after` is set → ASC LIMIT directly.
///
/// Limits: clamped to `[1, 1000]`. Returning 5000+ rows is the slice-8
/// anti-pattern this slice replaces — see DEV-S8-14.
pub async fn list_session_window(
    pool: &SqlitePool,
    session_id: &str,
    before: Option<&Cursor>,
    after: Option<&Cursor>,
    limit: i64,
) -> Result<Vec<ObservedEvent>> {
    let limit = limit.clamp(1, 1000);
    let rows = match (before, after) {
        (None, None) => {
            // Conversation-anchored initial window. The message view renders
            // only conversation/activity kinds and DROPS bulk telemetry
            // (metric_sample / otel_span / most log_record) and non-rendered
            // envelopes (attachment_meta / session_state). A session that ends
            // in a telemetry burst would otherwise load a newest-N raw window
            // full of dropped rows → empty stream though conversation exists.
            //
            // So window by RENDERED events, not raw: bound the window to
            //   [ limit-th newest rendered event ,  newest rendered event ]
            // and return every row in that range (interleaved telemetry stays
            // loaded for detail metrics; the trailing burst above the newest
            // rendered event is excluded). Guarantees up to `limit` rendered
            // events, or the whole session when it has fewer. Falls back to
            // plain newest-N when the session has no rendered events at all.
            const RENDERED: &str = "kind IN ('user_message','assistant_message',\
                'tool_call','tool_result','thinking','hook_event','system_summary','diff_hunk')";
            // newest rendered event = the upper bound (anchor).
            let upper = sqlx::query(&format!(
                "SELECT observed_at, event_id FROM observed_event \
                 WHERE session_id = ? AND {RENDERED} \
                 ORDER BY observed_at DESC, event_id DESC LIMIT 1"
            ))
            .bind(session_id)
            .fetch_optional(pool)
            .await?;
            match upper {
                None => {
                    // No rendered events (telemetry-only session) → newest-N.
                    sqlx::query(
                        "SELECT * FROM observed_event WHERE session_id = ? \
                         ORDER BY observed_at DESC, event_id DESC LIMIT ?",
                    )
                    .bind(session_id)
                    .bind(limit)
                    .fetch_all(pool)
                    .await?
                }
                Some(u) => {
                    let uts: String = u.get("observed_at");
                    let ueid: String = u.get("event_id");
                    // `limit`-th newest rendered event = the lower bound (None
                    // when the session has fewer than `limit` rendered events,
                    // in which case the window runs back to the session start).
                    let lower = sqlx::query(&format!(
                        "SELECT observed_at, event_id FROM observed_event \
                         WHERE session_id = ? AND {RENDERED} \
                         ORDER BY observed_at DESC, event_id DESC LIMIT 1 OFFSET ?"
                    ))
                    .bind(session_id)
                    .bind(limit - 1)
                    .fetch_optional(pool)
                    .await?;
                    // Raw cap so a pathologically telemetry-dense range can't
                    // return an unbounded page (mirrors the client's max window).
                    const RAW_CAP: i64 = 5000;
                    match lower {
                        Some(l) => {
                            let lts: String = l.get("observed_at");
                            let leid: String = l.get("event_id");
                            sqlx::query(
                                "SELECT * FROM observed_event WHERE session_id = ? \
                                 AND (observed_at < ? OR (observed_at = ? AND event_id <= ?)) \
                                 AND (observed_at > ? OR (observed_at = ? AND event_id >= ?)) \
                                 ORDER BY observed_at DESC, event_id DESC LIMIT ?",
                            )
                            .bind(session_id)
                            .bind(&uts)
                            .bind(&uts)
                            .bind(&ueid)
                            .bind(&lts)
                            .bind(&lts)
                            .bind(&leid)
                            .bind(RAW_CAP)
                            .fetch_all(pool)
                            .await?
                        }
                        None => sqlx::query(
                            "SELECT * FROM observed_event WHERE session_id = ? \
                             AND (observed_at < ? OR (observed_at = ? AND event_id <= ?)) \
                             ORDER BY observed_at DESC, event_id DESC LIMIT ?",
                        )
                        .bind(session_id)
                        .bind(&uts)
                        .bind(&uts)
                        .bind(&ueid)
                        .bind(RAW_CAP)
                        .fetch_all(pool)
                        .await?,
                    }
                }
            }
        }
        (Some(b), None) => {
            let ts = b.observed_at.to_rfc3339();
            sqlx::query(
                "SELECT * FROM observed_event WHERE session_id = ? \
                 AND (observed_at < ? OR (observed_at = ? AND event_id < ?)) \
                 ORDER BY observed_at DESC, event_id DESC LIMIT ?",
            )
            .bind(session_id)
            .bind(&ts)
            .bind(&ts)
            .bind(&b.event_id)
            .bind(limit)
            .fetch_all(pool)
            .await?
        }
        (None, Some(a)) => {
            let ts = a.observed_at.to_rfc3339();
            sqlx::query(
                "SELECT * FROM observed_event WHERE session_id = ? \
                 AND (observed_at > ? OR (observed_at = ? AND event_id > ?)) \
                 ORDER BY observed_at ASC, event_id ASC LIMIT ?",
            )
            .bind(session_id)
            .bind(&ts)
            .bind(&ts)
            .bind(&a.event_id)
            .bind(limit)
            .fetch_all(pool)
            .await?
        }
        (Some(b), Some(a)) => {
            let ats = a.observed_at.to_rfc3339();
            let bts = b.observed_at.to_rfc3339();
            sqlx::query(
                "SELECT * FROM observed_event WHERE session_id = ? \
                 AND (observed_at > ? OR (observed_at = ? AND event_id > ?)) \
                 AND (observed_at < ? OR (observed_at = ? AND event_id < ?)) \
                 ORDER BY observed_at ASC, event_id ASC LIMIT ?",
            )
            .bind(session_id)
            .bind(&ats)
            .bind(&ats)
            .bind(&a.event_id)
            .bind(&bts)
            .bind(&bts)
            .bind(&b.event_id)
            .bind(limit)
            .fetch_all(pool)
            .await?
        }
    };
    let mut events: Vec<ObservedEvent> = rows.into_iter().map(row_to_observed).collect();
    // before-only and no-cursor used DESC SQL — flip to chronological ASC.
    let needs_reverse = matches!((before, after), (Some(_), None) | (None, None));
    if needs_reverse {
        events.reverse();
    }
    Ok(events)
}

/// On-demand correlated telemetry for the detail view: the events whose indexed
/// `tool_use_id` / `request_id` columns match the given keys, used when an
/// entity's correlated telemetry falls outside the loaded message window.
///
/// Correlation uses the INDEXED columns (not payload JSON) introduced in C2:
///
///   **tool_use_id arm** — `kind != 'tool_call'` guard is intentional:
///     - OTel `log_record` / `metric_sample` events: column set by C2 ingest
///     - transcript `tool_result`: column set by `mapping.rs` (line 75-78)
///     - transcript `tool_call`: column set, but deliberately EXCLUDED — the
///       caller already holds the tool_call; returning it again would duplicate
///       it in the detail view.
///
///   **request_id arm** — scoped to OTel kinds only (`log_record`, `otel_span`,
///     `metric_sample`) to preserve the semantics of the original query, which
///     matched only `attributes.request_id` (OTel logs) and `raw_span.attributes`
///     (OTel spans — that `payload.raw_span` re-embed was since removed in
///     Tier 3-1; request_id now lives in the indexed column). Transcript
///     `assistant_message` / `thinking` / `tool_call` also carry `request_id` in
///     the column (set by `mapping.rs`), but they were NOT matched by the old
///     payload-path query and must remain excluded.
pub async fn events_correlated(
    pool: &SqlitePool,
    session_id: &str,
    tool_use_id: Option<&str>,
    request_id: Option<&str>,
) -> Result<Vec<ObservedEvent>> {
    let rows = sqlx::query(
        "SELECT * FROM observed_event WHERE session_id = ? AND ( \
           (? IS NOT NULL AND tool_use_id = ? AND kind != 'tool_call') \
           OR (? IS NOT NULL AND request_id = ? AND kind IN ('log_record','otel_span','metric_sample')) \
         ) ORDER BY observed_at ASC, event_id ASC LIMIT 500",
    )
    .bind(session_id)
    .bind(tool_use_id)
    .bind(tool_use_id)
    .bind(request_id)
    .bind(request_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(row_to_observed).collect())
}

/// Slice-9 — per-kind row counts for a single session. Replaces the
/// summary's `by_kind` that slice-8 derived from the (windowed) events array;
/// once `session_detail` stopped returning events, by_kind needed its own
/// query so the WebUI's source-mix badges stay accurate on 5000+ event
/// sessions.
pub async fn session_kind_counts(
    pool: &SqlitePool,
    session_id: &str,
) -> Result<std::collections::BTreeMap<String, i64>> {
    let rows = sqlx::query(
        "SELECT kind, COUNT(*) AS n FROM observed_event \
         WHERE session_id = ? GROUP BY kind",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?;
    let mut out = std::collections::BTreeMap::new();
    for r in rows {
        let k: String = r.get("kind");
        let n: i64 = r.get("n");
        out.insert(k, n);
    }
    Ok(out)
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
            "attachment_meta" => EventKind::AttachmentMeta,
            "otel_span" => EventKind::OtelSpan,
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
