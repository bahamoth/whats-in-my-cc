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

#[tokio::test]
async fn usage_endpoint_returns_public_pricing_estimate() {
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

    // Fixture is all claude-opus-4-7 (priced) with non-zero tokens → positive.
    assert!(
        data["estimated_cost_usd"].as_f64().unwrap() > 0.0,
        "real fixture should yield a positive public-pricing estimate"
    );
    // Never presented as actual billing.
    assert_eq!(data["cost_basis"].as_str().unwrap(), "estimate_public_pricing");
    assert_eq!(data["pricing_version"].as_str().unwrap(), "pricing_estimate@v1");
    // claude-opus-4-7 is in the table → nothing unpriced for this fixture.
    assert!(data["models_without_pricing"].as_array().unwrap().is_empty());

    // Per-model detail carries the token split + per-model cost.
    let by_model = data["by_model"].as_array().unwrap();
    assert!(!by_model.is_empty());
    let m0 = &by_model[0];
    assert_eq!(m0["model"].as_str().unwrap(), "claude-opus-4-7");
    assert!(m0["priced"].as_bool().unwrap());
    assert!(m0["cache_read_input_tokens"].as_i64().unwrap() > 0);
    assert!(m0["estimated_cost_usd"].as_f64().unwrap() > 0.0);
}
