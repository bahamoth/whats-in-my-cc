//! Slice-19 — Red-locking tests: /v1/audit endpoint.
//!
//! These tests will FAIL until `GET /v1/audit` is implemented.

use axum_test::TestServer;
use serde_json::json;

async fn build_auth_server_with_pool() -> (TestServer, sqlx::SqlitePool, String) {
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("WITMCC_CONFIG_DIR", dir.path());
    let token = witmcc::security::token::ensure_token().unwrap();
    std::env::remove_var("WITMCC_CONFIG_DIR");

    let pool = witmcc::db::connect(":memory:").await.unwrap();
    witmcc::db::migrate(&pool).await.unwrap();

    let mut state = witmcc::api::AppState::new_for_tests(pool.clone());
    state.token = token.clone();

    let app = witmcc::api::router(state);
    let server = TestServer::new(app).unwrap();
    (server, pool, token)
}

#[tokio::test]
async fn audit_endpoint_requires_auth() {
    let (server, _, _) = build_auth_server_with_pool().await;
    let r = server.get("/v1/audit").await;
    r.assert_status_unauthorized();
}

#[tokio::test]
async fn audit_endpoint_returns_rows() {
    let (server, pool, token) = build_auth_server_with_pool().await;

    sqlx::query(
        "INSERT INTO audit (audit_id, event, actor, payload) VALUES ('aud_1', 'api.accessed', 'owner', '{}')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let r = server
        .get("/v1/audit")
        .add_header(
            axum::http::header::AUTHORIZATION,
            &format!("Bearer {token}"),
        )
        .await;
    r.assert_status_ok();
    let body: serde_json::Value = r.json();
    let data = body["data"].as_array().unwrap();
    assert!(data.len() >= 1, "audit endpoint should return at least 1 row");
    assert_eq!(data[0]["event"], json!("api.accessed"));
}

#[tokio::test]
async fn audit_endpoint_returns_empty_when_no_rows() {
    let (server, _, token) = build_auth_server_with_pool().await;
    let r = server
        .get("/v1/audit")
        .add_header(
            axum::http::header::AUTHORIZATION,
            &format!("Bearer {token}"),
        )
        .await;
    r.assert_status_ok();
    let body: serde_json::Value = r.json();
    let data = body["data"].as_array().unwrap();
    assert_eq!(data.len(), 0, "audit endpoint should return empty array when no rows");
}
