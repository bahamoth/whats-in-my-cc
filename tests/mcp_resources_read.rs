//! Slice-17 — resources/read tests (red-locking).

use axum_test::TestServer;
use serde_json::{json, Value};
use sqlx::sqlite::SqlitePoolOptions;
use witmcc::db::migrate;
use witmcc::ingest::store;
use witmcc::graph::build;

async fn make_server_with_session() -> TestServer {
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    store::ingest_file(
        &pool,
        std::path::Path::new("tests/fixtures/transcripts/minimal_session.jsonl"),
        &witmcc::live::NoopSink,
    )
    .await
    .unwrap();
    build::rebuild_session(&pool, "sess-A").await.unwrap();

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

fn sid_header(sid: &str) -> (axum::http::HeaderName, axum::http::HeaderValue) {
    (
        axum::http::HeaderName::from_static("mcp-session-id"),
        axum::http::HeaderValue::from_str(sid).unwrap(),
    )
}

#[tokio::test]
async fn resources_read_session_returns_contents_with_uri() {
    let server = make_server_with_session().await;
    let sid = init_session(&server).await;
    let (hk, hv) = sid_header(&sid);
    let r = server
        .post("/mcp")
        .content_type("application/json")
        .add_header(hk, hv)
        .json(&json!({
            "jsonrpc": "2.0", "id": 20, "method": "resources/read",
            "params": { "uri": "whats-in-my-cc://sessions/sess-A" }
        }))
        .await;
    r.assert_status_ok();
    let body: Value = r.json();
    let contents = body["result"]["contents"].as_array().expect("contents must be array");
    assert!(!contents.is_empty(), "contents must not be empty");
    let first = &contents[0];
    assert_eq!(first["uri"], "whats-in-my-cc://sessions/sess-A");
    assert_eq!(first["mimeType"], "application/json");
    let text_str = first["text"].as_str().expect("text must be string");
    let parsed: Value = serde_json::from_str(text_str).expect("text must be valid JSON");
    assert!(parsed["data"].is_object(), "data must be an object");
}

#[tokio::test]
async fn resources_read_graph_returns_nodes_and_edges() {
    let server = make_server_with_session().await;
    let sid = init_session(&server).await;
    let (hk, hv) = sid_header(&sid);
    let r = server
        .post("/mcp")
        .content_type("application/json")
        .add_header(hk, hv)
        .json(&json!({
            "jsonrpc": "2.0", "id": 21, "method": "resources/read",
            "params": { "uri": "whats-in-my-cc://sessions/sess-A/graph" }
        }))
        .await;
    r.assert_status_ok();
    let body: Value = r.json();
    let contents = body["result"]["contents"].as_array().unwrap();
    assert!(!contents.is_empty());
    let text_str = contents[0]["text"].as_str().unwrap();
    let parsed: Value = serde_json::from_str(text_str).unwrap();
    assert!(parsed["data"]["nodes"].is_array());
    assert!(parsed["data"]["edges"].is_array());
}

#[tokio::test]
async fn resources_read_unknown_uri_returns_is_error() {
    let server = make_server_with_session().await;
    let sid = init_session(&server).await;
    let (hk, hv) = sid_header(&sid);
    let r = server
        .post("/mcp")
        .content_type("application/json")
        .add_header(hk, hv)
        .json(&json!({
            "jsonrpc": "2.0", "id": 22, "method": "resources/read",
            "params": { "uri": "whats-in-my-cc://unknown/xyz" }
        }))
        .await;
    r.assert_status_ok();
    let body: Value = r.json();
    // Unknown resource URI: either error in JSON-RPC error field, or isError in result
    let has_error = !body["error"].is_null()
        || body["result"]["isError"].as_bool().unwrap_or(false)
        || body["result"]["contents"].as_array().map(|a| a.is_empty()).unwrap_or(true);
    assert!(has_error, "unknown URI must result in an error response");
}
