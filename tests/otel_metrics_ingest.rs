//! Slice-6 Stage 1 — POST /otel/v1/metrics raw receiver integration tests.

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
    let app = witmcc::api::router(pool.clone());
    let server = TestServer::new(app).unwrap();
    (server, pool)
}

fn fixture_bytes(path: &str) -> Vec<u8> {
    std::fs::read(path).unwrap()
}

async fn metrics_raw_count(pool: &sqlx::SqlitePool) -> i64 {
    let row: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM raw_event WHERE source_type = 'otel-metrics'")
            .fetch_one(pool)
            .await
            .unwrap();
    row.0
}

#[tokio::test]
async fn post_metrics_minimal_returns_200_and_stores_one_raw_row() {
    let (s, pool) = http_setup().await;
    let body = fixture_bytes("tests/fixtures/otel/metrics/minimal.json");
    let resp = s
        .post("/otel/v1/metrics")
        .add_header("content-type", "application/json")
        .bytes(body.into())
        .await;
    resp.assert_status_ok();
    let v: serde_json::Value = resp.json();
    assert_eq!(v["meta"]["schema_version"], "0.5.0");
    assert_eq!(v["data"]["accepted_resource_metrics"], 1);
    assert_eq!(v["data"]["stored_raw_rows"], 1);
    assert_eq!(v["data"]["duplicate_raw_rows"], 0);
    assert_eq!(metrics_raw_count(&pool).await, 1);
}

#[tokio::test]
async fn post_metrics_twice_dedupes_via_payload_sha() {
    let (s, pool) = http_setup().await;
    let body = fixture_bytes("tests/fixtures/otel/metrics/minimal.json");

    let _ = s
        .post("/otel/v1/metrics")
        .add_header("content-type", "application/json")
        .bytes(body.clone().into())
        .await;
    let resp2 = s
        .post("/otel/v1/metrics")
        .add_header("content-type", "application/json")
        .bytes(body.into())
        .await;
    resp2.assert_status_ok();
    let v: serde_json::Value = resp2.json();
    assert_eq!(v["data"]["stored_raw_rows"], 0);
    assert_eq!(v["data"]["duplicate_raw_rows"], 1);
    assert_eq!(metrics_raw_count(&pool).await, 1);
}

#[tokio::test]
async fn post_metrics_non_json_body_is_400() {
    let (s, pool) = http_setup().await;
    let resp = s
        .post("/otel/v1/metrics")
        .add_header("content-type", "application/json")
        .bytes(b"not json".to_vec().into())
        .await;
    assert_eq!(resp.status_code(), 400);
    assert_eq!(metrics_raw_count(&pool).await, 0);
}

#[tokio::test]
async fn post_metrics_gzip_body_decompressed_by_middleware() {
    let (s, pool) = http_setup().await;
    let body = fixture_bytes("tests/fixtures/otel/metrics/minimal.json");

    let mut enc = GzEncoder::new(Vec::new(), Compression::default());
    enc.write_all(&body).unwrap();
    let gz_body = enc.finish().unwrap();

    let resp = s
        .post("/otel/v1/metrics")
        .add_header("content-type", "application/json")
        .add_header("content-encoding", "gzip")
        .bytes(gz_body.into())
        .await;
    resp.assert_status_ok();
    let v: serde_json::Value = resp.json();
    assert_eq!(v["data"]["accepted_resource_metrics"], 1);
    assert_eq!(v["data"]["stored_raw_rows"], 1);
    assert_eq!(metrics_raw_count(&pool).await, 1);
}
