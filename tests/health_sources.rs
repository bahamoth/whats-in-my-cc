//! Slice-6 — `/v1/health/sources` taxonomy + freshness tests.

use axum_test::TestServer;
use sqlx::sqlite::SqlitePoolOptions;
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

async fn http_setup() -> TestServer {
    let pool = make_pool().await;
    let app = witmcc::api::router(witmcc::api::AppState::new_for_tests(pool));
    TestServer::new(app).unwrap()
}

#[tokio::test]
async fn health_sources_returns_full_taxonomy_when_db_empty() {
    let s = http_setup().await;
    let resp = s.get("/v1/health/sources").await;
    resp.assert_status_ok();
    let v: serde_json::Value = resp.json();
    let sources = v["data"]["sources"].as_array().unwrap();
    let labels: Vec<&str> = sources
        .iter()
        .map(|s| s["label"].as_str().unwrap())
        .collect();
    assert_eq!(
        labels,
        vec!["transcript", "otel-traces", "otel-metrics", "otel-logs", "hook", "file-git"]
    );
    for src in sources {
        assert!(src["last_ingested_at"].is_null());
        assert_eq!(src["row_count_24h"], 0);
        assert_eq!(src["total_rows"], 0);
    }
}

#[tokio::test]
async fn health_sources_reports_recency_after_ingest() {
    let s = http_setup().await;
    let body = std::fs::read("tests/fixtures/otel/real/metrics_v01.json").unwrap();
    s.post("/otel/v1/metrics")
        .add_header("content-type", "application/json")
        .bytes(body.into())
        .await
        .assert_status_ok();

    let resp = s.get("/v1/health/sources").await;
    let v: serde_json::Value = resp.json();
    let metrics = v["data"]["sources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["label"] == "otel-metrics")
        .unwrap();
    assert!(metrics["last_ingested_at"].is_string());
    assert!(metrics["total_rows"].as_i64().unwrap() >= 1);
    assert!(metrics["row_count_24h"].as_i64().unwrap() >= 1);
}
