//! Slice-17 — Protocol compat golden test (red-locking).
//!
//! Asserts that the MCP protocol shape (initialize response + tools/list +
//! resources/templates/list) matches the frozen golden in
//! tests/fixtures/mcp/protocol_compat.json.
//!
//! Bumping requires updating the golden AND adding a deviation note.

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
                "clientInfo": {"name": "compat-test", "version": "1.0"}
            }
        }))
        .await;
    r.header("Mcp-Session-Id").to_str().unwrap().to_string()
}

#[tokio::test]
async fn protocol_compat_initialize_shape() {
    let compat_path = "tests/fixtures/mcp/protocol_compat.json";
    let compat_str = std::fs::read_to_string(compat_path)
        .expect("tests/fixtures/mcp/protocol_compat.json must exist");
    let compat: Value =
        serde_json::from_str(&compat_str).expect("compat fixture must be valid JSON");

    let expected_initialize = &compat["initialize_response"];

    let server = make_server().await;
    let r = server
        .post("/mcp")
        .content_type("application/json")
        .json(&json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "compat-test", "version": "1.0"}
            }
        }))
        .await;
    let body: Value = r.json();

    // Compare the structural shape (capabilities, protocolVersion)
    // Placeholders: serverInfo.version is dynamic, Mcp-Session-Id is dynamic.
    assert_eq!(
        body["result"]["protocolVersion"], expected_initialize["result"]["protocolVersion"],
        "protocolVersion must match compat fixture"
    );
    assert_eq!(
        body["result"]["capabilities"], expected_initialize["result"]["capabilities"],
        "capabilities must match compat fixture"
    );
    assert_eq!(
        body["result"]["serverInfo"]["name"], expected_initialize["result"]["serverInfo"]["name"],
        "serverInfo.name must match compat fixture"
    );
}

#[tokio::test]
async fn protocol_compat_tools_list_shape() {
    let compat_str = std::fs::read_to_string("tests/fixtures/mcp/protocol_compat.json")
        .expect("compat fixture must exist");
    let compat: Value = serde_json::from_str(&compat_str).unwrap();
    let expected_tools = compat["tools_list_result"]["tools"]
        .as_array()
        .expect("compat fixture must have tools_list_result.tools");

    let server = make_server().await;
    let sid = init_session(&server).await;
    let r = server
        .post("/mcp")
        .content_type("application/json")
        .add_header(
            axum::http::HeaderName::from_static("mcp-session-id"),
            axum::http::HeaderValue::from_str(&sid).unwrap(),
        )
        .json(&json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}))
        .await;
    let body: Value = r.json();
    let got_tools = body["result"]["tools"].as_array().unwrap();

    let mut got_names: Vec<&str> = got_tools
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    let mut exp_names: Vec<&str> = expected_tools
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    got_names.sort();
    exp_names.sort();

    assert_eq!(
        got_names, exp_names,
        "tools/list tool names must match compat fixture"
    );
}

#[tokio::test]
async fn protocol_compat_resource_templates_shape() {
    let compat_str = std::fs::read_to_string("tests/fixtures/mcp/protocol_compat.json")
        .expect("compat fixture must exist");
    let compat: Value = serde_json::from_str(&compat_str).unwrap();
    let expected_templates = compat["resource_templates_result"]["resourceTemplates"]
        .as_array()
        .expect("compat fixture must have resource_templates_result.resourceTemplates");

    let server = make_server().await;
    let sid = init_session(&server).await;
    let r = server
        .post("/mcp")
        .content_type("application/json")
        .add_header(
            axum::http::HeaderName::from_static("mcp-session-id"),
            axum::http::HeaderValue::from_str(&sid).unwrap(),
        )
        .json(&json!({"jsonrpc": "2.0", "id": 3, "method": "resources/templates/list"}))
        .await;
    let body: Value = r.json();
    let got = body["result"]["resourceTemplates"].as_array().unwrap();

    let mut got_uris: Vec<&str> = got
        .iter()
        .map(|t| t["uriTemplate"].as_str().unwrap())
        .collect();
    let mut exp_uris: Vec<&str> = expected_templates
        .iter()
        .map(|t| t["uriTemplate"].as_str().unwrap())
        .collect();
    got_uris.sort();
    exp_uris.sort();

    assert_eq!(
        got_uris, exp_uris,
        "resource templates must match compat fixture"
    );
}
