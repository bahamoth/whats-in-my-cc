//! Slice-17 — tools/list compat fixture test (red-locking).
//!
//! The tools/list response must match the golden fixture at
//! tests/fixtures/mcp/tools_list_expected.json. Any change to tool names or
//! input schemas must update the fixture in the same commit.

use axum_test::TestServer;
use serde_json::{json, Value};
use sqlx::sqlite::SqlitePoolOptions;
use wimcc::db::migrate;

async fn make_server() -> TestServer {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    let state = wimcc::api::AppState::new_for_tests(pool);
    TestServer::new(wimcc::api::router(state)).unwrap()
}

/// POST initialize, return Mcp-Session-Id header value.
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
async fn tools_list_returns_five_tools() {
    let server = make_server().await;
    let sid = init_session(&server).await;
    let r = server
        .post("/mcp")
        .content_type("application/json")
        .add_header(
            axum::http::HeaderName::from_static("mcp-session-id"),
            axum::http::HeaderValue::from_str(&sid).unwrap(),
        )
        .json(&json!({"jsonrpc": "2.0", "id": 3, "method": "tools/list"}))
        .await;
    r.assert_status_ok();
    let body: Value = r.json();
    let tools = body["result"]["tools"]
        .as_array()
        .expect("tools must be an array");
    assert_eq!(
        tools.len(),
        5,
        "expected exactly 5 tools (dogfood 2026-06-12: +get_session_turns), got {}",
        tools.len()
    );
}

#[tokio::test]
async fn tools_list_contains_required_tool_names() {
    let server = make_server().await;
    let sid = init_session(&server).await;
    let r = server
        .post("/mcp")
        .content_type("application/json")
        .add_header(
            axum::http::HeaderName::from_static("mcp-session-id"),
            axum::http::HeaderValue::from_str(&sid).unwrap(),
        )
        .json(&json!({"jsonrpc": "2.0", "id": 4, "method": "tools/list"}))
        .await;
    let body: Value = r.json();
    let tools = body["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();

    let required = [
        "whats_in_my_cc.get_file_lineage",
        "whats_in_my_cc.get_otel_trace",
        "whats_in_my_cc.search_sessions",
        "whats_in_my_cc.list_detectors",
    ];
    for name in &required {
        assert!(
            names.contains(name),
            "tools/list missing tool '{name}', got: {names:?}"
        );
    }
}

#[tokio::test]
async fn tools_list_matches_compat_fixture() {
    let fixture_path = "tests/fixtures/mcp/tools_list_expected.json";
    let expected_str = std::fs::read_to_string(fixture_path)
        .expect("tests/fixtures/mcp/tools_list_expected.json must exist");
    let expected: Value = serde_json::from_str(&expected_str).expect("fixture must be valid JSON");

    let server = make_server().await;
    let sid = init_session(&server).await;
    let r = server
        .post("/mcp")
        .content_type("application/json")
        .add_header(
            axum::http::HeaderName::from_static("mcp-session-id"),
            axum::http::HeaderValue::from_str(&sid).unwrap(),
        )
        .json(&json!({"jsonrpc": "2.0", "id": 5, "method": "tools/list"}))
        .await;
    let body: Value = r.json();
    let got = &body["result"]["tools"];

    // We sort both arrays by name so ordering differences don't cause spurious failures.
    let mut got_tools: Vec<Value> = got.as_array().unwrap().to_vec();
    let mut exp_tools: Vec<Value> = expected["tools"].as_array().unwrap().to_vec();
    got_tools.sort_by_key(|t| t["name"].as_str().unwrap_or("").to_string());
    exp_tools.sort_by_key(|t| t["name"].as_str().unwrap_or("").to_string());

    assert_eq!(
        got_tools, exp_tools,
        "tools/list shape diverged from fixture. Update tests/fixtures/mcp/tools_list_expected.json if intentional."
    );
}
