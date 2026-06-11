//! Slice-19 — Red-locking tests: bearer-token middleware on /v1 and /mcp.
//!
//! These tests will FAIL until `src/api/middleware/auth.rs` is wired into
//! the router and AppState carries a `token` field.

use axum_test::TestServer;
use serde_json::json;

/// Build a test server with auth enabled.
/// The returned string is the bearer token to use.
/// Token is generated in-process (no file I/O) to avoid env-var races.
async fn build_auth_test_server() -> (TestServer, String) {
    // Use generate_token() directly — no file system interaction.
    let token = wimcc::security::token::generate_token();

    let pool = wimcc::db::connect(":memory:").await.unwrap();
    wimcc::db::migrate(&pool).await.unwrap();

    let mut state = wimcc::api::AppState::new_for_tests(pool);
    state.token = token.clone();

    let app = wimcc::api::router(state);
    let server = TestServer::new(app).unwrap();
    (server, token)
}

#[tokio::test]
async fn rejects_request_without_bearer() {
    let (server, _) = build_auth_test_server().await;
    let r = server.get("/v1/health").await;
    r.assert_status_unauthorized();
}

#[tokio::test]
async fn accepts_request_with_correct_bearer() {
    let (server, token) = build_auth_test_server().await;
    let r = server
        .get("/v1/health")
        .add_header(axum::http::header::AUTHORIZATION, format!("Bearer {token}"))
        .await;
    r.assert_status_ok();
}

#[tokio::test]
async fn rejects_request_with_wrong_bearer() {
    let (server, _) = build_auth_test_server().await;
    let r = server
        .get("/v1/health")
        .add_header(axum::http::header::AUTHORIZATION, "Bearer wrong_token_xyz")
        .await;
    r.assert_status_unauthorized();
}

#[tokio::test]
async fn health_endpoint_has_security_block() {
    let (server, token) = build_auth_test_server().await;
    let r = server
        .get("/v1/health")
        .add_header(axum::http::header::AUTHORIZATION, format!("Bearer {token}"))
        .await;
    r.assert_status_ok();
    let body: serde_json::Value = r.json();
    assert!(
        body.get("security").is_some(),
        "health response should contain a 'security' block, got: {body}"
    );
    let sec = &body["security"];
    assert_eq!(
        sec["auth_required"],
        json!(true),
        "security.auth_required should be true"
    );
    assert!(
        sec.get("retention_profile").is_some(),
        "security.retention_profile should be present"
    );
}

#[tokio::test]
async fn mcp_endpoint_also_requires_token() {
    let (server, _) = build_auth_test_server().await;
    let r = server
        .post("/mcp")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "t", "version": "0"}
            }
        }))
        .await;
    r.assert_status_unauthorized();
}

#[tokio::test]
async fn mcp_endpoint_accepts_correct_bearer() {
    let (server, token) = build_auth_test_server().await;
    let r = server
        .post("/mcp")
        .add_header(axum::http::header::AUTHORIZATION, format!("Bearer {token}"))
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "t", "version": "0"}
            }
        }))
        .await;
    r.assert_status_ok();
}
