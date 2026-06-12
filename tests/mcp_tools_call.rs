//! Slice-17 — tools/call integration tests (red-locking).
//!
//! Each tool is called with minimal valid arguments and the response
//! shape is asserted. The data content is secondary to the shape.

use axum_test::TestServer;
use serde_json::{json, Value};
use sqlx::sqlite::SqlitePoolOptions;
use wimcc::db::migrate;
use wimcc::ingest::store;

async fn make_server_with_session() -> (TestServer, sqlx::SqlitePool) {
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    store::ingest_file(
        &pool,
        std::path::Path::new("tests/fixtures/transcripts/minimal_session.jsonl"),
        &wimcc::live::NoopSink,
    )
    .await
    .unwrap();

    let state = wimcc::api::AppState::new_for_tests(pool.clone());
    let server = TestServer::new(wimcc::api::router(state)).unwrap();
    (server, pool)
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
async fn search_sessions_returns_data_array() {
    let (server, _pool) = make_server_with_session().await;
    let sid = init_session(&server).await;
    let (hk, hv) = sid_header(&sid);
    let r = server
        .post("/mcp")
        .content_type("application/json")
        .add_header(hk, hv)
        .json(&json!({
            "jsonrpc": "2.0", "id": 11, "method": "tools/call",
            "params": {
                "name": "whats_in_my_cc.search_sessions",
                "arguments": { "limit": 10 }
            }
        }))
        .await;
    r.assert_status_ok();
    let body: Value = r.json();
    assert_eq!(body["result"]["isError"], false);
    let text = body["result"]["content"][0]["text"].as_str().unwrap();
    let env: Value = serde_json::from_str(text).unwrap();
    assert!(env["data"].is_array(), "search_sessions data must be array");
}

#[tokio::test]
async fn get_file_lineage_returns_content_block() {
    let (server, _pool) = make_server_with_session().await;
    let sid = init_session(&server).await;
    let (hk, hv) = sid_header(&sid);
    let r = server
        .post("/mcp")
        .content_type("application/json")
        .add_header(hk, hv)
        .json(&json!({
            "jsonrpc": "2.0", "id": 14, "method": "tools/call",
            "params": {
                "name": "whats_in_my_cc.get_file_lineage",
                "arguments": { "session_id": "sess-A", "file_path": "src/main.rs" }
            }
        }))
        .await;
    r.assert_status_ok();
    let body: Value = r.json();
    assert!(body["result"]["content"].is_array());
}

#[tokio::test]
async fn get_otel_trace_returns_content_block() {
    let (server, _pool) = make_server_with_session().await;
    let sid = init_session(&server).await;
    let (hk, hv) = sid_header(&sid);
    let r = server
        .post("/mcp")
        .content_type("application/json")
        .add_header(hk, hv)
        .json(&json!({
            "jsonrpc": "2.0", "id": 15, "method": "tools/call",
            "params": {
                "name": "whats_in_my_cc.get_otel_trace",
                "arguments": { "trace_id": "0000000000000000ffffffffffffffff" }
            }
        }))
        .await;
    r.assert_status_ok();
    let body: Value = r.json();
    assert!(body["result"]["content"].is_array());
}

#[tokio::test]
async fn list_detectors_returns_four_manifests() {
    let (server, _pool) = make_server_with_session().await;
    let sid = init_session(&server).await;
    let (hk, hv) = sid_header(&sid);
    let r = server
        .post("/mcp")
        .content_type("application/json")
        .add_header(hk, hv)
        .json(&json!({
            "jsonrpc": "2.0", "id": 20, "method": "tools/call",
            "params": {
                "name": "whats_in_my_cc.list_detectors",
                "arguments": {}
            }
        }))
        .await;
    r.assert_status_ok();
    let body: Value = r.json();
    assert_eq!(
        body["result"]["isError"], false,
        "list_detectors must not return isError"
    );
    let text = body["result"]["content"][0]["text"].as_str().unwrap();
    let env: Value = serde_json::from_str(text).unwrap();
    let data = env["data"]
        .as_array()
        .expect("list_detectors data must be array");
    assert_eq!(
        data.len(),
        5,
        "list_detectors must return 5 manifests; got {}",
        data.len()
    );
    // Verify each manifest has id and intent.
    for m in data {
        assert!(m["id"].is_string(), "manifest.id must be a string");
        assert!(m["intent"].is_string(), "manifest.intent must be a string");
    }
}

#[tokio::test]
async fn get_project_metrics_returns_series() {
    let (server, _pool) = make_server_with_session().await;
    let sid = init_session(&server).await;
    let (hk, hv) = sid_header(&sid);
    let r = server
        .post("/mcp")
        .content_type("application/json")
        .add_header(hk, hv)
        .json(&json!({
            "jsonrpc": "2.0", "id": 21, "method": "tools/call",
            "params": {
                "name": "whats_in_my_cc.get_project_metrics",
                "arguments": { "limit": 5 }
            }
        }))
        .await;
    r.assert_status_ok();
    let body: Value = r.json();
    assert_eq!(body["result"]["isError"], false);
    let text = body["result"]["content"][0]["text"].as_str().unwrap();
    let payload: Value = serde_json::from_str(text).unwrap();
    assert!(payload["data"]["sessions"].is_array());
    assert!(payload["data"]["matched_count"].is_i64());
    // minimal_session.jsonl이 ingest되어 있으므로 row 형태도 잠근다.
    let rows = payload["data"]["sessions"].as_array().unwrap();
    assert!(!rows.is_empty(), "ingested session must appear in series");
    assert!(rows[0]["metrics"]["tool_call_total"].is_i64());
    assert!(rows[0]["fingerprint"]["models"].is_array());
}

#[tokio::test]
async fn unknown_tool_name_returns_is_error_true() {
    let (server, _pool) = make_server_with_session().await;
    let sid = init_session(&server).await;
    let (hk, hv) = sid_header(&sid);
    let r = server
        .post("/mcp")
        .content_type("application/json")
        .add_header(hk, hv)
        .json(&json!({
            "jsonrpc": "2.0", "id": 99, "method": "tools/call",
            "params": {
                "name": "not_a_real_tool",
                "arguments": {}
            }
        }))
        .await;
    r.assert_status_ok();
    let body: Value = r.json();
    assert_eq!(body["result"]["isError"], true);
}
