//! doc-audit-2026-06-10 — `GET /v1/events/:id/raw` must report the DB
//! `raw_event.redaction_state` (doc 05 vocabulary: redacted | not_redacted |
//! not_applicable), not a hardcoded `"none"`. Legacy rows ingested before
//! Slice-18 have a NULL column value; the API surfaces those as JSON null
//! (no value in the doc 05 vocabulary describes "scan never ran", so we do
//! not fabricate one).

use axum_test::TestServer;
use serde_json::Value;
use sqlx::sqlite::SqlitePoolOptions;
use wimcc::db::migrate;
use wimcc::ingest::store;
use wimcc::live::NoopSink;

async fn make_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    pool
}

fn build_server(pool: sqlx::SqlitePool) -> TestServer {
    let state = wimcc::api::AppState::new_for_tests(pool);
    TestServer::new(wimcc::api::router(state)).unwrap()
}

/// A raw row the redaction engine marked `redacted` (synthetic secrets
/// fixture) must surface that exact state through the raw endpoint.
#[tokio::test]
async fn event_raw_reports_redacted_state_from_db() {
    let pool = make_pool().await;
    store::ingest_file(
        &pool,
        std::path::Path::new("tests/fixtures/redaction/synthetic_secrets.jsonl"),
        &NoopSink,
    )
    .await
    .unwrap();

    let event_id: String = sqlx::query_scalar(
        "SELECT o.event_id FROM observed_event o \
         JOIN raw_event r ON r.raw_event_id = o.raw_event_id \
         WHERE r.redaction_state = 'redacted' LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .expect("synthetic secrets fixture must produce a redacted raw row");

    let server = build_server(pool);
    let resp = server.get(&format!("/v1/events/{event_id}/raw")).await;
    resp.assert_status_ok();
    let body: Value = resp.json();
    assert_eq!(
        body["data"]["redaction_state"], "redacted",
        "redaction_state must mirror raw_event.redaction_state; got: {}",
        body["data"]["redaction_state"]
    );
}

/// Legacy raw rows (pre-Slice-18) have NULL redaction_state in the DB.
/// The endpoint must return JSON null for them — never a fabricated state.
#[tokio::test]
async fn event_raw_reports_null_for_legacy_rows_without_state() {
    let pool = make_pool().await;

    sqlx::query(
        "INSERT INTO ingest_run(run_id, started_at, status) \
         VALUES('run-legacy', '2026-01-01T00:00:00Z', 'completed')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO raw_event(\
            raw_event_id, ingest_run_id, source_type, source_uri, \
            source_line_no, source_byte_offset, payload_sha256, payload, \
            parse_error, captured_at, redaction_state, redaction_manifest) \
         VALUES('raw-legacy', 'run-legacy', 'claude_transcript', '/tmp/legacy.jsonl', \
            1, 0, 'sha-legacy', X'7B7D', \
            NULL, '2026-01-01T00:00:00Z', NULL, NULL)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO observed_event(\
            event_id, raw_event_id, schema_version, session_id, observed_at, \
            actor, kind, payload, parser_version) \
         VALUES('evt-legacy', 'raw-legacy', '0.5.0', 'sess-legacy', \
            '2026-01-01T00:00:00Z', 'user', 'user_message', '{}', 'test')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let server = build_server(pool);
    let resp = server.get("/v1/events/evt-legacy/raw").await;
    resp.assert_status_ok();
    let body: Value = resp.json();
    assert!(
        body["data"]["redaction_state"].is_null(),
        "NULL redaction_state rows must surface as JSON null; got: {}",
        body["data"]["redaction_state"]
    );
}
