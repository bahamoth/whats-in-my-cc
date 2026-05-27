//! Slice-17 — GET /mcp SSE channel test (red-locking).
//!
//! The SSE channel must:
//! - return Content-Type: text/event-stream
//! - emit a keepalive comment within the keepalive window
//!
//! Full notifications/resources/updated testing is done via subprocess
//! (L5 test) because axum-test's TestServer doesn't support concurrent
//! request-while-SSE-streaming in the same tokio test easily. This file
//! covers the basic connectivity shape.

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

async fn init_session(server: &TestServer) -> String {
    let r = server
        .post("/mcp")
        .content_type("application/json")
        .json(&json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "t", "version": "0"}
            }
        }))
        .await;
    r.header("Mcp-Session-Id").to_str().unwrap().to_string()
}

#[tokio::test]
async fn mcp_sse_endpoint_returns_text_event_stream() {
    let server = make_server().await;
    let sid = init_session(&server).await;

    // GET /mcp with Mcp-Session-Id and Accept: text/event-stream
    let r = server
        .get("/mcp")
        .add_header(
            axum::http::HeaderName::from_static("mcp-session-id"),
            axum::http::HeaderValue::from_str(&sid).unwrap(),
        )
        .add_header(
            axum::http::header::ACCEPT,
            axum::http::HeaderValue::from_static("text/event-stream"),
        )
        .await;
    r.assert_status_ok();
    let ct = r.header("content-type");
    assert!(
        ct.to_str().unwrap().contains("text/event-stream"),
        "GET /mcp must return Content-Type: text/event-stream, got {:?}",
        ct
    );
}

#[tokio::test]
async fn mcp_sse_unknown_session_returns_404() {
    let server = make_server().await;
    let r = server
        .get("/mcp")
        .add_header(
            axum::http::HeaderName::from_static("mcp-session-id"),
            axum::http::HeaderValue::from_static("mcps_nonexistent_session_id"),
        )
        .add_header(
            axum::http::header::ACCEPT,
            axum::http::HeaderValue::from_static("text/event-stream"),
        )
        .await;
    r.assert_status(axum::http::StatusCode::NOT_FOUND);
}
