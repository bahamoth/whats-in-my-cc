//! GET /v1/usage/baseline returns the cross-session median (+ p25/p75) of the
//! key usage metrics. Seeds two sessions with known values and asserts the
//! median is computed correctly in Rust (SQLite has no MEDIAN()).
use axum_test::TestServer;
use serde_json::Value;
use sqlx::sqlite::SqlitePoolOptions;
use wimcc::api::{router, AppState};
use wimcc::db::repo_usage_facet::UsageFacetRow;
use wimcc::db::{migrate, repo_usage_facet};

async fn empty_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    pool
}

fn uf(
    raw_event_id: &str,
    session_id: &str,
    input: i64,
    cc: i64,
    cr: i64,
    output: i64,
) -> UsageFacetRow {
    UsageFacetRow {
        raw_event_id: raw_event_id.into(),
        schema_version: "usage_facet.v1".into(),
        session_id: session_id.into(),
        model: Some("claude-opus-4-8".into()),
        input_tokens: input,
        cache_creation_input_tokens: cc,
        cache_read_input_tokens: cr,
        output_tokens: output,
        observed_at: "2026-05-30T10:00:00Z".into(),
        parser_version: "usage_facet@v1".into(),
    }
}

#[tokio::test]
async fn baseline_endpoint_returns_median_across_sessions() {
    let pool = empty_pool().await;

    // Session 1: turns=1, billed = 100 + 0 + 100 = 200, output 100,
    //   denom = cr(0)+cc(0)+input(100)=100, cache_hit_ratio = 0/100 = 0.0
    repo_usage_facet::insert(&pool, &uf("r1", "sess_lo", 100, 0, 0, 100))
        .await
        .unwrap();
    // Session 2: turns=1, billed = 100 + 0 + 300 = 400, output 300,
    //   denom = cr(900)+cc(0)+input(100)=1000, cache_hit_ratio = 900/1000 = 0.9
    repo_usage_facet::insert(&pool, &uf("r2", "sess_hi", 100, 0, 900, 300))
        .await
        .unwrap();

    let state = AppState::new_for_tests(pool);
    let server = TestServer::new(router(state)).unwrap();
    let r = server.get("/v1/usage/baseline").await;
    r.assert_status_ok();
    let body = r.json::<Value>();
    let data = &body["data"];

    assert_eq!(data["session_count"].as_i64().unwrap(), 2);
    // Two values -> median is the midpoint (type-7 interpolation).
    // billed_tokens: [200, 400] -> median 300.
    assert_eq!(data["billed_tokens"]["median"].as_f64().unwrap(), 300.0);
    // output_tokens: [100, 300] -> median 200.
    assert_eq!(data["output_tokens"]["median"].as_f64().unwrap(), 200.0);
    // assistant_events: [1, 1] -> median 1.
    assert_eq!(data["assistant_events"]["median"].as_f64().unwrap(), 1.0);
    // cache_hit_ratio: [0.0, 0.9] -> median 0.45.
    let chr = data["cache_hit_ratio"]["median"].as_f64().unwrap();
    assert!((chr - 0.45).abs() < 1e-9, "got {chr}");
    // p25/p75 present (not null) when there is data.
    assert!(data["billed_tokens"]["p25"].as_f64().is_some());
    assert!(data["billed_tokens"]["p75"].as_f64().is_some());
}

#[tokio::test]
async fn baseline_endpoint_empty_store_returns_nulls() {
    let pool = empty_pool().await;
    let state = AppState::new_for_tests(pool);
    let server = TestServer::new(router(state)).unwrap();
    let r = server.get("/v1/usage/baseline").await;
    r.assert_status_ok();
    let body = r.json::<Value>();
    let data = &body["data"];
    assert_eq!(data["session_count"].as_i64().unwrap(), 0);
    assert!(data["billed_tokens"]["median"].is_null());
    assert!(data["cache_hit_ratio"]["median"].is_null());
}
