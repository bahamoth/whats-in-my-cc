//! Slice-8 L2 integration tests for SSE wiring.
//!
//! Termination strategy — axum-test 16 fully-buffers responses, so a 200
//! status on the SSE endpoint hangs forever (keepalive never closes the
//! body). Instead we bypass axum-test for streaming cases and call the
//! router as a tower `Service` directly via `ServiceExt::oneshot`, then
//! read body frames one at a time with `http_body_util::BodyExt::frame()`.
//! Each test reads chunks until it sees the expected substring (success),
//! or hits a per-frame timeout (failure). The body is dropped on success,
//! which closes the stream cleanly.
//!
//! What this gives us:
//! - Real HTTP handshake (status + headers) inspected per request.
//! - Real chunked body iteration → backfill / live forward / resync /
//!   filter behaviour exercised in-process, fast, deterministic.
//! - Hard timeout per frame → no test ever hangs longer than the budget.

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use sqlx::sqlite::SqlitePoolOptions;
use tokio::sync::broadcast;
use tower::ServiceExt;
use wimcc::api::AppState;
use wimcc::db::migrate;
use wimcc::ingest::store;
use wimcc::live::{BroadcastSink, LiveEvent};
use wimcc::model::observed::EventKind;

const FRAME_TIMEOUT: Duration = Duration::from_secs(2);

async fn setup() -> (sqlx::SqlitePool, AppState) {
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    let (tx, _) = broadcast::channel::<LiveEvent>(512);
    let state = AppState {
        pool: pool.clone(),
        live_tx: Arc::new(tx),
        sse_keepalive_secs: 30,
        sse_channel_capacity: 512,
        mcp_sessions: wimcc::api::mcp::SessionRegistry::new(),
        // Slice-19: empty token disables auth check in test mode.
        token: String::new(),
        retention_profile: "none".to_string(),
        shutdown: tokio_util::sync::CancellationToken::new(),
        update_status: Default::default(),
        sweep_stats: Default::default(),
        db_path: None,
    };
    (pool, state)
}

/// Send an HTTP request to `app` and read body frames until `expected` appears
/// in the accumulated UTF-8 buffer. Returns `(status, bytes_seen)` on success.
/// Panics with a clear message if the per-frame timeout fires or the stream
/// closes without seeing `expected`.
async fn read_until(
    app: axum::Router,
    request: Request<Body>,
    expected: &str,
) -> (StatusCode, String) {
    let resp = app.oneshot(request).await.expect("oneshot");
    let status = resp.status();
    let headers = resp.headers().clone();
    let mut body = resp.into_body();
    let mut acc = Vec::new();
    loop {
        let frame = tokio::time::timeout(FRAME_TIMEOUT, body.frame())
            .await
            .unwrap_or_else(|_| {
                panic!(
                    "no frame arrived within {:?}; status={} headers={:?}; saw so far: {:?}",
                    FRAME_TIMEOUT,
                    status,
                    headers,
                    String::from_utf8_lossy(&acc).into_owned()
                )
            });
        let frame = match frame {
            Some(Ok(f)) => f,
            Some(Err(e)) => panic!("body error: {e}"),
            None => panic!(
                "stream closed without seeing {expected:?}; status={} headers={:?}; saw: {:?}",
                status,
                headers,
                String::from_utf8_lossy(&acc).into_owned()
            ),
        };
        if let Some(chunk) = frame.data_ref() {
            acc.extend_from_slice(chunk);
            if let Ok(s) = std::str::from_utf8(&acc) {
                if s.contains(expected) {
                    return (status, s.to_string());
                }
            }
        }
    }
}

/// Insert one ObservedEvent row with a known event_id. raw_event and
/// ingest_run rows are created first so the FK constraint is satisfied.
async fn insert_observed(
    pool: &sqlx::SqlitePool,
    event_id: &str,
    session_id: &str,
    kind: EventKind,
    observed_at: &str,
) {
    use wimcc::db::{repo_observed, repo_raw, repo_runs};
    use wimcc::model::observed::{Actor, ObservedEvent};

    let run_id = repo_runs::start(pool).await.expect("run start");
    let raw_id = format!("raw_{event_id}");
    repo_raw::insert_dedup(
        pool,
        &repo_raw::NewRaw {
            raw_event_id: raw_id.clone(),
            ingest_run_id: run_id,
            source_type: "test".into(),
            source_uri: format!("test://{event_id}"),
            source_line_no: 0,
            source_byte_offset: 0,
            payload_sha256: format!("sha_{event_id}"),
            payload: b"{}".to_vec(),
            parse_error: None,
            captured_at: chrono::Utc::now(),
            redaction_state: "not_applicable".into(),
            redaction_manifest: None,
        },
    )
    .await
    .expect("raw insert");

    let ev = ObservedEvent {
        event_id: event_id.into(),
        raw_event_id: raw_id,
        schema_version: "0.5.0".into(),
        session_id: session_id.into(),
        observed_at: chrono::DateTime::parse_from_rfc3339(observed_at)
            .unwrap()
            .with_timezone(&chrono::Utc),
        actor: Actor::User,
        kind,
        parser_version: "test".into(),
        ..Default::default()
    };
    repo_observed::insert(pool, &ev).await.expect("insert");
}

/// Production wiring guard for the transcript path: a real ingest_file call
/// must publish at least one envelope to AppState.live_tx subscribers.
#[tokio::test]
async fn transcript_ingest_publishes_to_live_tx() {
    let (pool, state) = setup().await;
    let mut rx = state.live_tx.subscribe();
    let sink = BroadcastSink::new(state.live_tx.clone());
    let path = std::path::Path::new("tests/fixtures/transcripts/minimal_session.jsonl");
    store::ingest_file(&pool, path, &sink).await.unwrap();
    let env = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("timed out waiting for envelope")
        .expect("broadcast recv");
    assert_eq!(env.source_type, "transcript");
    assert_eq!(env.schema_version, "1");
    assert_eq!(env.session_id, "sess-A");
}

/// Malformed Last-Event-ID returns HTTP 400 synchronously (no streaming).
#[tokio::test]
async fn last_event_id_malformed_returns_400() {
    let (_pool, state) = setup().await;
    let app = wimcc::api::router(state);
    let req = Request::builder()
        .uri("/v1/stream?last_event_id=with%0Acontrol")
        .header("host", "127.0.0.1")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

/// Empty Last-Event-ID header → 400.
#[tokio::test]
async fn last_event_id_header_empty_returns_400() {
    let (_pool, state) = setup().await;
    let app = wimcc::api::router(state);
    let req = Request::builder()
        .uri("/v1/stream")
        .header("host", "127.0.0.1")
        .header("last-event-id", "")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

/// Backfill — cursor at row A must emit row B (newer) but not A itself.
/// Reads the stream until row B's event_id is observed, then closes.
#[tokio::test]
async fn backfill_emits_only_rows_after_cursor() {
    let (pool, state) = setup().await;
    // Keep the broadcast Sender alive for the whole test. Without this, the
    // router move into `oneshot` drops the only Sender clones once the
    // handler future ends, BroadcastStream closes, and the body terminates
    // before backfill frames have flushed.
    let _tx_alive = state.live_tx.clone();
    insert_observed(
        &pool,
        "01HAAAAAAAAAAAAAAAAAAAAAAA",
        "sess",
        EventKind::UserMessage,
        "2026-05-21T10:00:00Z",
    )
    .await;
    insert_observed(
        &pool,
        "01HBBBBBBBBBBBBBBBBBBBBBBB",
        "sess",
        EventKind::AssistantMessage,
        "2026-05-21T10:00:01Z",
    )
    .await;

    let app = wimcc::api::router(state);
    let req = Request::builder()
        .uri("/v1/stream?last_event_id=01HAAAAAAAAAAAAAAAAAAAAAAA")
        .header("host", "127.0.0.1")
        .body(Body::empty())
        .unwrap();
    let (status, body) = read_until(app, req, "01HBBBBBBBBBBBBBBBBBBBBBBB").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        !body.contains("01HAAAAAAAAAAAAAAAAAAAAAAA"),
        "cursor row must not be re-emitted, got: {body}"
    );
}

/// Slice-6 DEV-S6-04 regression — metric/log event_ids use deterministic
/// `metric:<resource>:<instrument>:<time>:<attr>` format. The cursor
/// validator must accept them; otherwise EventSource gets 400 → infinite
/// reconnect loop with no data delivered.
#[tokio::test]
async fn metric_format_cursor_resumes_correctly() {
    let (pool, state) = setup().await;
    let _tx_alive = state.live_tx.clone();
    insert_observed(
        &pool,
        "metric:abc:claude_code.token.usage:1779340143371000000:c334b58e",
        "sess_metric",
        EventKind::MetricSample,
        "2026-05-21T10:00:00Z",
    )
    .await;
    insert_observed(
        &pool,
        "01HNEXTNEXTNEXTNEXTNEXTNXT",
        "sess_metric",
        EventKind::UserMessage,
        "2026-05-21T10:00:01Z",
    )
    .await;

    let app = wimcc::api::router(state);
    let req = Request::builder()
        .uri("/v1/stream?last_event_id=metric%3Aabc%3Aclaude_code.token.usage%3A1779340143371000000%3Ac334b58e")
        .header("host", "127.0.0.1")
        .body(Body::empty())
        .unwrap();
    let (status, _body) = read_until(app, req, "01HNEXTNEXTNEXTNEXTNEXTNXT").await;
    assert_eq!(status, StatusCode::OK);
}

/// Live forward — broadcast::send a LiveEvent and confirm it shows up in
/// the stream body. Connects FIRST (no cursor → no backfill), then publishes.
#[tokio::test]
async fn live_envelope_arrives_via_stream() {
    let (_pool, state) = setup().await;
    let tx_alive = state.live_tx.clone();
    let app = wimcc::api::router(state);

    // Spawn the publisher; clone the Arc again so the spawned task has its
    // own handle and the original Arc in this scope keeps the channel open
    // regardless of when the task ends.
    let tx_for_task = tx_alive.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let _ = tx_for_task.send(LiveEvent {
            schema_version: "1".into(),
            session_id: "sess_live".into(),
            event_id: "01HLIVELIVELIVELIVELIVELIV".into(),
            kind: EventKind::UserMessage,
            source_type: "transcript".into(),
            observed_at: "2026-05-21T10:00:00Z".into(),
        });
    });

    let req = Request::builder()
        .uri("/v1/stream")
        .header("host", "127.0.0.1")
        .body(Body::empty())
        .unwrap();
    let (status, body) = read_until(app, req, "01HLIVELIVELIVELIVELIVELIV").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("\"source_type\":\"transcript\""));
    drop(tx_alive);
}

/// Resync — well-formed cursor that does not exist in DB triggers
/// `event: resync` as the first frame before any backfill / live data.
#[tokio::test]
async fn unknown_cursor_emits_resync_frame() {
    let (_pool, state) = setup().await;
    let _tx_alive = state.live_tx.clone();
    let app = wimcc::api::router(state);
    let req = Request::builder()
        .uri("/v1/stream?last_event_id=01HZZZZZZZZZZZZZZZZZZZZZZA")
        .header("host", "127.0.0.1")
        .body(Body::empty())
        .unwrap();
    let (status, body) = read_until(app, req, "event: resync").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("unknown_cursor"));
}

/// Session filter — a stream opened with `?session=A` must not receive
/// envelopes broadcast for session B. Verified by waiting for an event
/// from session A while session B is also being published.
#[tokio::test]
async fn session_filter_drops_other_sessions() {
    let (_pool, state) = setup().await;
    let tx_alive = state.live_tx.clone();
    let app = wimcc::api::router(state);

    let tx_for_task = tx_alive.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        // Wrong session — must be filtered out.
        let _ = tx_for_task.send(LiveEvent {
            schema_version: "1".into(),
            session_id: "sess_OTHER".into(),
            event_id: "01HOTHEROTHEROTHEROTHEROTH".into(),
            kind: EventKind::UserMessage,
            source_type: "transcript".into(),
            observed_at: "2026-05-21T10:00:00Z".into(),
        });
        // Right session — must come through.
        tokio::time::sleep(Duration::from_millis(50)).await;
        let _ = tx_for_task.send(LiveEvent {
            schema_version: "1".into(),
            session_id: "sess_WANTED".into(),
            event_id: "01HWANTEDWANTEDWANTEDWANTED".into(),
            kind: EventKind::UserMessage,
            source_type: "transcript".into(),
            observed_at: "2026-05-21T10:00:01Z".into(),
        });
    });

    let req = Request::builder()
        .uri("/v1/stream?session=sess_WANTED")
        .header("host", "127.0.0.1")
        .body(Body::empty())
        .unwrap();
    let (status, body) = read_until(app, req, "01HWANTEDWANTEDWANTEDWANTED").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        !body.contains("01HOTHEROTHEROTHEROTHEROTH"),
        "envelope from wrong session leaked through filter: {body}"
    );
    drop(tx_alive);
}
