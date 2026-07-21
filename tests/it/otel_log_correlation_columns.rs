//! C1 — OTel log records whose OTLP attributes carry `tool_use_id` / `request_id`
//! must land those values in the indexed *columns* of `observed_event`, not only
//! inside the JSON payload blob.
//!
//! Fixture: `tests/fixtures/otel/logs/with_correlation_keys.json` — two log
//! records, one carrying both correlation keys, one carrying neither.

use axum_test::TestServer;
use sqlx::sqlite::SqlitePoolOptions;
use wimcc::db::migrate;

async fn make_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    pool
}

async fn http_setup() -> (TestServer, sqlx::SqlitePool) {
    let pool = make_pool().await;
    let app = wimcc::api::router(wimcc::api::AppState::new_for_tests(pool.clone()));
    let server = TestServer::new(app).unwrap();
    (server, pool)
}

/// Ingest the correlation-key fixture and assert that the log record whose
/// attributes contain `tool_use_id` + `request_id` populates the COLUMNS
/// (not just the JSON payload blob).
#[tokio::test]
async fn otel_log_record_attributes_tool_use_id_and_request_id_land_in_columns() {
    let (server, pool) = http_setup().await;

    let body = std::fs::read("tests/fixtures/otel/logs/with_correlation_keys.json").unwrap();
    let resp = server
        .post("/otel/v1/logs")
        .add_header("content-type", "application/json")
        .bytes(body.into())
        .await;
    resp.assert_status_ok();

    // Query the column directly — not via json_extract(payload, ...).
    // If the column is NULL the test fails, proving the current code doesn't promote.
    let rows: Vec<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT tool_use_id, request_id \
         FROM observed_event \
         WHERE kind = 'log_record' AND session_id = 'sess-corr-A' \
         ORDER BY observed_at ASC",
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(
        rows.len(),
        2,
        "expected 2 log_record rows; got {}",
        rows.len()
    );

    // First record carries both correlation keys.
    let (tool_use_id, request_id) = &rows[0];
    assert_eq!(
        tool_use_id.as_deref(),
        Some("toolu_abc123"),
        "tool_use_id column must be populated from attributes; got {tool_use_id:?}"
    );
    assert_eq!(
        request_id.as_deref(),
        Some("req_xyz789"),
        "request_id column must be populated from attributes; got {request_id:?}"
    );

    // Second record carries neither — columns must stay NULL (no clobbering).
    let (tool_use_id2, request_id2) = &rows[1];
    assert!(
        tool_use_id2.is_none(),
        "log record without tool_use_id attribute must have NULL column; got {tool_use_id2:?}"
    );
    assert!(
        request_id2.is_none(),
        "log record without request_id attribute must have NULL column; got {request_id2:?}"
    );
}
