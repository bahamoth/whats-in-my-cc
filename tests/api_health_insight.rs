//! /v1/health must include status "ok" and a security block.
//! The judge counters were removed when the LLM judge subsystem was deleted.

use axum_test::TestServer;
use wimcc::api::AppState;

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
    wimcc::db::migrate(&pool).await.unwrap();
    TestServer::new(wimcc::api::router(AppState::new_for_tests(pool))).unwrap()
}

#[tokio::test]
async fn health_returns_ok() {
    let srv = test_server().await;
    let r = srv.get("/v1/health").await;
    r.assert_status_ok();
    let body: serde_json::Value = r.json();
    assert_eq!(body["status"], "ok", "status must be 'ok'");
}

#[tokio::test]
async fn health_includes_security_block() {
    let srv = test_server().await;
    let r = srv.get("/v1/health").await;
    let body: serde_json::Value = r.json();
    assert!(
        body["security"].is_object(),
        "security block missing from health response"
    );
    assert_eq!(
        body["security"]["auth_required"], false,
        "auth_required must be false in test mode (empty token)"
    );
}
