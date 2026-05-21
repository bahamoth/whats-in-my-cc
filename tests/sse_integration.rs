//! Slice-8 L2 integration tests for SSE wiring.
//!
//! In-process tests for things that do NOT require reading a streaming HTTP
//! body. axum-test 16 buffers the full response, which would hang on a
//! long-lived SSE connection. Streaming behaviour (backfill order, dedup,
//! gap on Lagged, keepalive, subscriber lifecycle) is verified end-to-end
//! in `tests/sse_subprocess.rs` against a real `witmcc serve` process with
//! reqwest streaming.
//!
//! What this file covers:
//! - Production wiring guard: ingest writers reach `AppState.live_tx`
//!   subscribers via the BroadcastSink (no HTTP, direct subscribe).
//! - Cursor validation: malformed `Last-Event-ID` returns HTTP 400 fast
//!   (synchronous error path, no streaming).

use std::sync::Arc;
use std::time::Duration;

use sqlx::sqlite::SqlitePoolOptions;
use tokio::sync::broadcast;
use witmcc::api::AppState;
use witmcc::db::migrate;
use witmcc::ingest::store;
use witmcc::live::{BroadcastSink, LiveEvent};

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
    };
    (pool, state)
}

/// Production wiring guard for the transcript path: a real ingest_file call
/// must publish at least one envelope to AppState.live_tx subscribers. If
/// any future refactor accidentally drops the BroadcastSink in production
/// paths this test goes red (subprocess L3 tests cover the other paths).
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

/// Malformed Last-Event-ID returns HTTP 400 synchronously (no streaming) so
/// axum-test can verify status without hanging.
#[tokio::test]
async fn last_event_id_malformed_returns_400() {
    let (_pool, state) = setup().await;
    let app = witmcc::api::router(state);
    let server = axum_test::TestServer::new(app).unwrap();
    let resp = server.get("/v1/stream?last_event_id=NOT-A-ULID").await;
    assert_eq!(resp.status_code(), axum::http::StatusCode::BAD_REQUEST);
}

/// Last-Event-ID via header (instead of query) also rejected when malformed.
#[tokio::test]
async fn last_event_id_header_malformed_returns_400() {
    let (_pool, state) = setup().await;
    let app = witmcc::api::router(state);
    let server = axum_test::TestServer::new(app).unwrap();
    let resp = server
        .get("/v1/stream")
        .add_header("last-event-id", "TOO-SHORT")
        .await;
    assert_eq!(resp.status_code(), axum::http::StatusCode::BAD_REQUEST);
}
