//! Slice-6 — POST /otel/v1/logs receiver integration tests.
//!
//! Stage 1 (raw_event idempotency, gzip, bad-json) plus Stage 2 (LogRecord
//! ObservedEvents + graph nodes anchored on the real Claude Code fixture).

use axum_test::TestServer;
use flate2::write::GzEncoder;
use flate2::Compression;
use sqlx::sqlite::SqlitePoolOptions;
use std::io::Write;
use witmcc::db::migrate;

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
    let app = witmcc::api::router(witmcc::api::AppState::new_for_tests(pool.clone()));
    let server = TestServer::new(app).unwrap();
    (server, pool)
}

fn fixture_bytes(path: &str) -> Vec<u8> {
    std::fs::read(path).unwrap()
}

async fn logs_raw_count(pool: &sqlx::SqlitePool) -> i64 {
    let row: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM raw_event WHERE source_type = 'otel-logs'")
            .fetch_one(pool)
            .await
            .unwrap();
    row.0
}

#[tokio::test]
async fn post_logs_minimal_returns_200_and_stores_one_raw_row() {
    let (s, pool) = http_setup().await;
    let body = fixture_bytes("tests/fixtures/otel/logs/minimal.json");
    let resp = s
        .post("/otel/v1/logs")
        .add_header("content-type", "application/json")
        .bytes(body.into())
        .await;
    resp.assert_status_ok();
    let v: serde_json::Value = resp.json();
    assert_eq!(v["meta"]["schema_version"], "0.5.0");
    assert_eq!(v["data"]["accepted_resource_logs"], 1);
    assert_eq!(v["data"]["stored_raw_rows"], 1);
    assert_eq!(v["data"]["duplicate_raw_rows"], 0);
    assert_eq!(logs_raw_count(&pool).await, 1);
}

#[tokio::test]
async fn post_logs_twice_dedupes_via_payload_sha() {
    let (s, pool) = http_setup().await;
    let body = fixture_bytes("tests/fixtures/otel/logs/minimal.json");

    let _ = s
        .post("/otel/v1/logs")
        .add_header("content-type", "application/json")
        .bytes(body.clone().into())
        .await;
    let resp2 = s
        .post("/otel/v1/logs")
        .add_header("content-type", "application/json")
        .bytes(body.into())
        .await;
    resp2.assert_status_ok();
    let v: serde_json::Value = resp2.json();
    assert_eq!(v["data"]["stored_raw_rows"], 0);
    assert_eq!(v["data"]["duplicate_raw_rows"], 1);
    assert_eq!(logs_raw_count(&pool).await, 1);
}

#[tokio::test]
async fn post_logs_non_json_body_is_400() {
    let (s, pool) = http_setup().await;
    let resp = s
        .post("/otel/v1/logs")
        .add_header("content-type", "application/json")
        .bytes(b"not json".to_vec().into())
        .await;
    assert_eq!(resp.status_code(), 400);
    assert_eq!(logs_raw_count(&pool).await, 0);
}

async fn observed_log_record_count(pool: &sqlx::SqlitePool) -> i64 {
    let row: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM observed_event WHERE kind = 'log_record'")
            .fetch_one(pool)
            .await
            .unwrap();
    row.0
}

// Slice 2 (telemetry fold): the real logs fixture carries *orphan* log records
// (hook_execution_complete + mcp_server_connection — none of which is a foldable
// tool_result/tool_decision/api_request). They are normalised into observed_event
// (SSOT) but dropped from the graph.
#[tokio::test]
async fn post_logs_real_fixture_normalises_records_to_observed_event_not_graph() {
    let (s, pool) = http_setup().await;
    let body = fixture_bytes("tests/fixtures/otel/real/logs_v01.json");
    let resp = s
        .post("/otel/v1/logs")
        .add_header("content-type", "application/json")
        .bytes(body.into())
        .await;
    resp.assert_status_ok();
    let v: serde_json::Value = resp.json();
    let accepted = v["data"]["accepted_log_records"].as_u64().unwrap_or(0);
    assert!(accepted >= 1, "real fixture has ≥1 log record; got {accepted}");
    let touched = v["data"]["sessions_touched"].as_array().unwrap();
    assert_eq!(touched.len(), 1);
    let session_id = touched[0].as_str().unwrap().to_string();

    // SSOT: every accepted log record is an observed_event row.
    assert_eq!(observed_log_record_count(&pool).await, accepted as i64);

    // Graph: zero log_record nodes after the Slice-2 drop (these are orphan logs).
    let graph_row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM graph_node WHERE session_id = ? AND node_kind = 'log_record'",
    )
    .bind(&session_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(graph_row.0, 0, "Slice 2: orphan log_record dropped from graph");
}

#[tokio::test]
async fn post_logs_real_fixture_twice_dedupes_records() {
    let (s, pool) = http_setup().await;
    let body = fixture_bytes("tests/fixtures/otel/real/logs_v01.json");
    let _ = s
        .post("/otel/v1/logs")
        .add_header("content-type", "application/json")
        .bytes(body.clone().into())
        .await;
    let first = observed_log_record_count(&pool).await;
    assert!(first > 0);

    let resp2 = s
        .post("/otel/v1/logs")
        .add_header("content-type", "application/json")
        .bytes(body.into())
        .await;
    resp2.assert_status_ok();
    let v: serde_json::Value = resp2.json();
    assert_eq!(v["data"]["stored_raw_rows"], 0);
    assert_eq!(v["data"]["accepted_log_records"], 0);
    assert!(v["data"]["duplicate_log_records"].as_u64().unwrap() >= 1);
    assert_eq!(observed_log_record_count(&pool).await, first);
}

#[tokio::test]
async fn post_logs_gzip_body_decompressed_by_middleware() {
    let (s, pool) = http_setup().await;
    let body = fixture_bytes("tests/fixtures/otel/logs/minimal.json");

    let mut enc = GzEncoder::new(Vec::new(), Compression::default());
    enc.write_all(&body).unwrap();
    let gz_body = enc.finish().unwrap();

    let resp = s
        .post("/otel/v1/logs")
        .add_header("content-type", "application/json")
        .add_header("content-encoding", "gzip")
        .bytes(gz_body.into())
        .await;
    resp.assert_status_ok();
    let v: serde_json::Value = resp.json();
    assert_eq!(v["data"]["accepted_resource_logs"], 1);
    assert_eq!(v["data"]["stored_raw_rows"], 1);
    assert_eq!(logs_raw_count(&pool).await, 1);
}
