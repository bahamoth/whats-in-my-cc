//! Slice-17 — MCP initialize handshake tests (red-locking).
//!
//! Tests the POST /mcp endpoint for `initialize` method:
//! - returns protocolVersion "2024-11-05"
//! - returns capabilities.tools + capabilities.resources.subscribe
//! - returns Mcp-Session-Id header starting with "mcps_"
//! - unknown method returns JSON-RPC -32601 error

use axum_test::TestServer;
use serde_json::{json, Value};
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

#[tokio::test]
async fn initialize_returns_protocol_version_and_session_id() {
    let server = make_server().await;
    let r = server
        .post("/mcp")
        .content_type("application/json")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "smoke", "version": "1.0" }
            }
        }))
        .await;
    r.assert_status_ok();
    let body: Value = r.json();
    assert_eq!(body["result"]["protocolVersion"], "2024-11-05");
    assert!(body["result"]["capabilities"]["tools"].is_object());
    assert_eq!(body["result"]["capabilities"]["resources"]["subscribe"], true);
    assert!(body["result"]["serverInfo"]["name"].is_string());

    let sid = r.header("Mcp-Session-Id");
    assert!(
        sid.to_str().unwrap().starts_with("mcps_"),
        "Mcp-Session-Id must start with 'mcps_', got: {:?}",
        sid
    );
}

#[tokio::test]
async fn unknown_method_returns_jsonrpc_method_not_found() {
    let server = make_server().await;
    let r = server
        .post("/mcp")
        .content_type("application/json")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "does_not_exist",
            "params": {}
        }))
        .await;
    r.assert_status_ok();
    let body: Value = r.json();
    assert_eq!(body["error"]["code"], -32601i32);
}

#[tokio::test]
async fn initialize_returns_prompts_capability() {
    let server = make_server().await;
    let r = server
        .post("/mcp")
        .content_type("application/json")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "t", "version": "0" }
            }
        }))
        .await;
    let body: Value = r.json();
    // prompts capability must be present (even if empty list)
    assert!(body["result"]["capabilities"]["prompts"].is_object());
}
