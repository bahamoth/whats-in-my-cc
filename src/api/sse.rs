//! Slice-8 — GET /v1/stream SSE handler.
//!
//! Spec: `docs/superpowers/specs/2026-05-21-wimcc-slice8-sse-design.md` §3 + §5.
//!
//! Subscribe-before-SELECT to close the race window; backfill from
//! `Last-Event-ID` cursor (header or `?last_event_id=` query) with
//! deduplication against forwarded broadcast envelopes. `event: gap` is
//! emitted on `BroadcastStreamRecvError::Lagged`; `event: resync` is the
//! first frame when a well-formed cursor matches no row (DB reset, retention
//! purge). Malformed cursor → HTTP 400.

use std::collections::HashSet;
use std::convert::Infallible;
use std::time::Duration;

use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::sse::{Event, KeepAlive, Sse},
};
use futures::stream::{self, Stream, StreamExt};
use serde::Deserialize;
use sqlx::Row;
use tokio_stream::wrappers::{errors::BroadcastStreamRecvError, BroadcastStream};

use crate::api::AppState;
use crate::live::LiveEvent;
use crate::model::observed::EventKind;

#[derive(Debug, Default, Deserialize)]
pub struct StreamQuery {
    pub session: Option<String>,
    pub last_event_id: Option<String>,
}

pub async fn stream_handler(
    State(state): State<AppState>,
    Query(q): Query<StreamQuery>,
    headers: HeaderMap,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, String)> {
    // Cursor: explicit query wins, otherwise Last-Event-ID header.
    let cursor = q.last_event_id.clone().or_else(|| {
        headers
            .get("last-event-id")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
    });

    if let Some(c) = cursor.as_deref() {
        if !is_well_formed_event_id(c) {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("malformed last-event-id: {c}"),
            ));
        }
    }

    // Step 1: subscribe FIRST so envelopes published between now and the SELECT
    // are buffered by the broadcast channel instead of lost.
    let rx = state.live_tx.subscribe();

    // Step 2: backfill via SELECT. resync_needed is true iff the client passed
    // a well-formed cursor that does not exist in observed_event.
    let (backfill_rows, resync_needed) =
        match load_backfill(&state.pool, cursor.as_deref(), q.session.as_deref()).await {
            Ok(v) => v,
            Err(e) => return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("backfill: {e}"))),
        };

    let seen_ids: HashSet<String> = backfill_rows.iter().map(|r| r.event_id.clone()).collect();

    // Pre-event stream: optional resync frame.
    let resync_frame: Option<Result<Event, Infallible>> = if resync_needed {
        Some(Ok(Event::default()
            .event("resync")
            .data(r#"{"reason":"unknown_cursor"}"#)))
    } else {
        None
    };
    let resync_stream = stream::iter(resync_frame.into_iter());

    // Backfill stream: one Event per row.
    let backfill_stream = stream::iter(backfill_rows.into_iter().map(|env| {
        let data = serde_json::to_string(&env).expect("LiveEvent serialise");
        Ok::<_, Infallible>(Event::default().id(env.event_id.clone()).data(data))
    }));

    // Live forward stream — filter by session + dedup against backfill.
    let session_filter = q.session.clone();
    let live_stream = BroadcastStream::new(rx).filter_map(move |item| {
        let session_filter = session_filter.clone();
        let seen_ids = seen_ids.clone();
        async move {
            match item {
                Ok(env) => {
                    if let Some(sid) = session_filter.as_deref() {
                        if env.session_id != sid {
                            return None;
                        }
                    }
                    if seen_ids.contains(&env.event_id) {
                        return None;
                    }
                    let data = serde_json::to_string(&env).ok()?;
                    Some(Ok::<_, Infallible>(
                        Event::default().id(env.event_id.clone()).data(data),
                    ))
                }
                Err(BroadcastStreamRecvError::Lagged(n)) => {
                    let payload = format!(r#"{{"dropped":{}}}"#, n);
                    Some(Ok(Event::default().event("gap").data(payload)))
                }
            }
        }
    });

    let combined = resync_stream.chain(backfill_stream).chain(live_stream);

    // Server-side cancellation: when the AppState.shutdown token fires
    // (Ctrl+C / graceful shutdown), terminate the stream so the
    // connection closes and axum can finish draining other handlers.
    // SSE has no DB writes — strictly read-only forwarding, so cutting
    // mid-stream is safe.
    let shutdown = state.shutdown.clone();
    let cancellable = combined.take_until(async move { shutdown.cancelled().await });

    Ok(Sse::new(cancellable).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(state.sse_keepalive_secs))
            .text("keepalive"),
    ))
}

fn is_well_formed_event_id(s: &str) -> bool {
    // event_id format is source-dependent. ULIDs (26 chars Crockford base32)
    // for transcripts/hooks/file_git, but slice-6 metric/log records use
    // deterministic ids like `metric:<resource>:<instrument>:<time>:<attr>` or
    // `log:<resource>:<time>:<trace>:<span>`. So we accept anything that looks
    // like a printable ASCII id of reasonable length. Truly malformed
    // (control chars, > 200 bytes) → 400; unknown-but-well-formed → resync.
    !s.is_empty()
        && s.len() <= 200
        && s.chars()
            .all(|c| c.is_ascii() && !c.is_ascii_control())
}

async fn load_backfill(
    pool: &sqlx::SqlitePool,
    cursor: Option<&str>,
    session: Option<&str>,
) -> Result<(Vec<LiveEvent>, bool), sqlx::Error> {
    // Spec §5 row 1: "No `Last-Event-ID` header, no `?last_event_id=` → Backfill = nothing."
    // Without this short-circuit the client receives the entire history (capped at 10k)
    // before the live stream catches up, which buries any recent envelope behind weeks
    // of old data. The baseline is already loaded via the existing GET endpoints.
    if cursor.is_none() {
        return Ok((Vec::new(), false));
    }

    let cursor_str = cursor.unwrap();
    // Lookup the cursor row's observed_at. event_id alone is NOT a sortable
    // key across sources because slice-6 metric/log ids
    // (`metric:<resource>:<instrument>:<time>:<attr>`) are not in the same
    // lexicographic order as ULIDs. Sorting + filtering by observed_at +
    // (event_id as tiebreaker) gives a monotonic resume order across all
    // sources without re-encoding event_id.
    let cursor_observed_at: Option<String> =
        sqlx::query_scalar("SELECT observed_at FROM observed_event WHERE event_id = ?")
            .bind(cursor_str)
            .fetch_optional(pool)
            .await?;
    let cursor_observed_at = match cursor_observed_at {
        Some(t) => t,
        None => return Ok((Vec::new(), true)), // resync
    };

    // Cursor exists — emit rows strictly newer in observed_at, plus rows at
    // the same instant with a higher event_id (so duplicate-tied timestamps
    // do not cause re-emission of the cursor row itself). LIMIT guards a
    // catastrophic multi-day backfill; past that, clients drop the cursor
    // and refetch baseline via GET.
    let mut q = String::from(
        "SELECT event_id, session_id, kind, observed_at \
         FROM observed_event \
         WHERE (observed_at > ? OR (observed_at = ? AND event_id > ?))",
    );
    if session.is_some() {
        q.push_str(" AND session_id = ?");
    }
    q.push_str(" ORDER BY observed_at ASC, event_id ASC LIMIT 10000");

    let mut query = sqlx::query(&q)
        .bind(&cursor_observed_at)
        .bind(&cursor_observed_at)
        .bind(cursor_str);
    if let Some(s) = session {
        query = query.bind(s);
    }
    let rows = query.fetch_all(pool).await?;

    let envs = rows
        .into_iter()
        .map(|r| {
            let kind_str: String = r.get("kind");
            let kind = parse_event_kind(&kind_str);
            let session_id: String = r.get("session_id");
            LiveEvent {
                schema_version: LiveEvent::SCHEMA_VERSION.to_string(),
                session_id,
                event_id: r.get("event_id"),
                kind,
                source_type: derive_source_type(kind),
                observed_at: r.get("observed_at"),
            }
        })
        .collect();
    Ok((envs, false))
}

fn parse_event_kind(s: &str) -> EventKind {
    match s {
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
    }
}

fn derive_source_type(kind: EventKind) -> String {
    match kind {
        EventKind::OtelSpan => "otel".into(),
        EventKind::MetricSample => "otel-metrics".into(),
        EventKind::LogRecord => "otel-logs".into(),
        EventKind::HookEvent => "hook".into(),
        _ => "transcript".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::is_well_formed_event_id as ok;

    #[test]
    fn metric_format_accepted() {
        // Slice-6 metric event_ids use `metric:<resource>:<instrument>:<time>:<attr>`
        // (DEV-S6-04). The validator must accept them, otherwise EventSource gets
        // 400 → onerror → reconnect loop with no data ever delivered. Observed live
        // on bahamoth's server when the WebUI sent
        // `last_event_id=metric:595247aa:claude_code.token.usage:...`.
        assert!(ok("metric:abc:claude_code.token.usage:1779340143371000000:c334b58e"));
        assert!(ok("log:abc:1779340143371000000:trace:span"));
    }

    #[test]
    fn ulid_accepted() {
        assert!(ok("01HZZZ000000000000000000A"));
    }

    #[test]
    fn rejected_inputs() {
        assert!(!ok(""));                  // empty
        assert!(!ok("with\ncontrol"));     // control char
        assert!(!ok(&"x".repeat(300)));    // too long
        assert!(!ok("non-ascii: 한글"));    // non-ASCII
    }
}
