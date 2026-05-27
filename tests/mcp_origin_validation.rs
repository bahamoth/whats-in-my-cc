//! Slice-17 — Origin header validation for /mcp endpoint (red-locking).
//!
//! Per design §5: if Origin is present and not in the allowlist
//! (http://127.0.0.1:* or http://localhost:*), respond 403.
//! If Origin is absent (curl-style), allow.

use axum_test::TestServer;
use serde_json::json;
use sqlx::sqlite::SqlitePoolOptions;
use witmcc::db::migrate;

async fn make_server() -> TestServer {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    let state = witmcc::api::AppState::new_for_tests(pool);
    TestServer::new(witmcc::api::router(state)).unwrap()
}

fn init_body() -> serde_json::Value {
    json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "t", "version": "0"}
        }
    })
}

#[tokio::test]
async fn rejects_disallowed_origin() {
    let server = make_server().await;
    let r = server
        .post("/mcp")
        .content_type("application/json")
        .add_header(
            axum::http::HeaderName::from_static("origin"),
            axum::http::HeaderValue::from_static("https://evil.example.com"),
        )
        .json(&init_body())
        .await;
    r.assert_status(axum::http::StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn allows_localhost_origin() {
    let server = make_server().await;
    let r = server
        .post("/mcp")
        .content_type("application/json")
        .add_header(
            axum::http::HeaderName::from_static("origin"),
            axum::http::HeaderValue::from_static("http://localhost:4337"),
        )
        .json(&init_body())
        .await;
    r.assert_status_ok();
}

#[tokio::test]
async fn allows_127_0_0_1_origin() {
    let server = make_server().await;
    let r = server
        .post("/mcp")
        .content_type("application/json")
        .add_header(
            axum::http::HeaderName::from_static("origin"),
            axum::http::HeaderValue::from_static("http://127.0.0.1:4337"),
        )
        .json(&init_body())
        .await;
    r.assert_status_ok();
}

#[tokio::test]
async fn allows_absent_origin_curl_style() {
    // No Origin header (curl-style) must be allowed (DEV-S17-07 context)
    let server = make_server().await;
    let r = server
        .post("/mcp")
        .content_type("application/json")
        .json(&init_body())
        .await;
    r.assert_status_ok();
}
