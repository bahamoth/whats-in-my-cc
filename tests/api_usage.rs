//! GET /v1/sessions/:id/usage returns the token-usage aggregate envelope.
use axum_test::TestServer;
use serde_json::Value;
use sqlx::sqlite::SqlitePoolOptions;
use witmcc::api::{router, AppState};
use witmcc::db::migrate;
use witmcc::ingest::store;
use witmcc::live::NoopSink;

async fn empty_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    pool
}

#[tokio::test]
async fn usage_endpoint_returns_aggregate() {
    let pool = empty_pool().await;
    store::ingest_file(
        &pool,
        std::path::Path::new("tests/fixtures/transcripts/real/verification_v01.jsonl"),
        &NoopSink,
    )
    .await
    .unwrap();

    let state = AppState::new_for_tests(pool);
    let server = TestServer::new(router(state)).unwrap();
    let r = server
        .get("/v1/sessions/aac68973-729e-4014-a02b-28a556f5ff29/usage")
        .await;
    r.assert_status_ok();
    let body = r.json::<Value>();
    let data = &body["data"];
    assert!(data["turns"].as_i64().unwrap() > 0);
    assert!(data["cache_read_input_tokens"].as_i64().unwrap() > 0);
    assert!(data["billed_tokens"].as_i64().unwrap() > 0);
    let chr = data["cache_hit_ratio"].as_f64().unwrap();
    assert!(chr > 0.0 && chr <= 1.0);
}
