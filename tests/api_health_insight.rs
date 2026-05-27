//! Slice-15 — /v1/health must include an `insight` block with judge counters.

use axum_test::TestServer;
use std::sync::Arc;
use witmcc::api::AppState;
use witmcc::insight::judge::runtime::JudgeRuntime;

async fn test_server() -> TestServer {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&pool)
        .await
        .unwrap();
    witmcc::db::migrate(&pool).await.unwrap();

    let (tx, _) = tokio::sync::broadcast::channel(64);
    let state = AppState {
        pool,
        live_tx: Arc::new(tx),
        sse_keepalive_secs: 30,
        sse_channel_capacity: 512,
        judge_runtime: Arc::new(JudgeRuntime::noop()),
        mcp_sessions: witmcc::api::mcp::SessionRegistry::new(),
        // Slice-19: empty token disables auth check in test mode.
        token: String::new(),
        retention_profile: "none".to_string(),
    };
    TestServer::new(witmcc::api::router(state)).unwrap()
}

#[tokio::test]
async fn health_includes_insight_block_with_judge_kind() {
    let srv = test_server().await;
    let r = srv.get("/v1/health").await;
    r.assert_status_ok();
    let body: serde_json::Value = r.json();
    assert!(
        body["insight"].is_object(),
        "insight block missing from health response"
    );
    assert_eq!(body["insight"]["judge_kind"], "noop");
}

#[tokio::test]
async fn health_insight_counters_are_numeric() {
    let srv = test_server().await;
    let r = srv.get("/v1/health").await;
    let body: serde_json::Value = r.json();
    for key in [
        "judge_calls_24h",
        "judge_pending_count",
        "judge_cache_hits_24h",
        "judge_cache_misses_24h",
        "judge_budget_exhaustions_24h",
    ] {
        assert!(
            body["insight"][key].is_number(),
            "insight.{key} is not a number: {}",
            body["insight"][key]
        );
    }
}

#[tokio::test]
async fn health_insight_noop_counters_are_zero() {
    let srv = test_server().await;
    let r = srv.get("/v1/health").await;
    let body: serde_json::Value = r.json();
    // With NoopJudge and no traffic, all 24h counters must be 0
    assert_eq!(body["insight"]["judge_calls_24h"], 0);
    assert_eq!(body["insight"]["judge_cache_hits_24h"], 0);
    assert_eq!(body["insight"]["judge_cache_misses_24h"], 0);
    assert_eq!(body["insight"]["judge_budget_exhaustions_24h"], 0);
}
