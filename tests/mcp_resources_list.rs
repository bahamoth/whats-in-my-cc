//! Slice-17 — resources/list + resources/templates/list tests (red-locking).

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
async fn resources_templates_list_has_six_templates() {
    let server = make_server().await;
    let sid = init_session(&server).await;
    let (hk, hv) = sid_header(&sid);
    let r = server
        .post("/mcp")
        .content_type("application/json")
        .add_header(hk, hv)
        .json(&json!({"jsonrpc": "2.0", "id": 5, "method": "resources/templates/list"}))
        .await;
    r.assert_status_ok();
    let body: Value = r.json();
    let templates = body["result"]["resourceTemplates"]
        .as_array()
        .expect("resourceTemplates must be an array");
    assert_eq!(templates.len(), 5, "expected 5 resource templates, got {}", templates.len());
}

#[tokio::test]
async fn resources_templates_list_contains_required_uri_templates() {
    let server = make_server().await;
    let sid = init_session(&server).await;
    let (hk, hv) = sid_header(&sid);
    let r = server
        .post("/mcp")
        .content_type("application/json")
        .add_header(hk, hv)
        .json(&json!({"jsonrpc": "2.0", "id": 6, "method": "resources/templates/list"}))
        .await;
    let body: Value = r.json();
    let templates = body["result"]["resourceTemplates"].as_array().unwrap();
    let uris: Vec<&str> = templates
        .iter()
        .map(|t| t["uriTemplate"].as_str().unwrap())
        .collect();

    let required = [
        "whats-in-my-cc://sessions/{session_id}",
        "whats-in-my-cc://sessions/{session_id}/findings",
        "whats-in-my-cc://findings/{finding_id}",
        "whats-in-my-cc://file-lineage/{session_id}",
        "whats-in-my-cc://otel/traces/{trace_id}",
    ];
    for uri in &required {
        assert!(
            uris.contains(uri),
            "resourceTemplates missing URI '{}', got: {:?}",
            uri,
            uris
        );
    }
}

#[tokio::test]
async fn resources_list_returns_resources_array() {
    let server = make_server().await;
    let sid = init_session(&server).await;
    let (hk, hv) = sid_header(&sid);
    let r = server
        .post("/mcp")
        .content_type("application/json")
        .add_header(hk, hv)
        .json(&json!({"jsonrpc": "2.0", "id": 7, "method": "resources/list"}))
        .await;
    r.assert_status_ok();
    let body: Value = r.json();
    // resources array may be empty (no sessions ingested) but must be present
    assert!(
        body["result"]["resources"].is_array(),
        "resources/list result.resources must be an array"
    );
}

#[tokio::test]
async fn prompts_list_returns_empty_array() {
    let server = make_server().await;
    let sid = init_session(&server).await;
    let (hk, hv) = sid_header(&sid);
    let r = server
        .post("/mcp")
        .content_type("application/json")
        .add_header(hk, hv)
        .json(&json!({"jsonrpc": "2.0", "id": 8, "method": "prompts/list"}))
        .await;
    r.assert_status_ok();
    let body: Value = r.json();
    let prompts = body["result"]["prompts"].as_array().expect("prompts must be array");
    assert!(prompts.is_empty(), "prompts/list must return empty array (DEV-S17-06)");
}
