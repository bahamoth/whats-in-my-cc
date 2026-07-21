//! Slice-18 — MCP resources/read must carry redaction annotations.

use axum_test::TestServer;
use serde_json::{json, Value};
use sqlx::sqlite::SqlitePoolOptions;
use wimcc::api::AppState;
use wimcc::db::migrate;
use wimcc::ingest::store;
use wimcc::live::NoopSink;

async fn make_server_with_session() -> (TestServer, String) {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    store::ingest_file(
        &pool,
        std::path::Path::new("tests/fixtures/redaction/synthetic_secrets.jsonl"),
        &NoopSink,
    )
    .await
    .unwrap();
    let sid: String = sqlx::query_scalar("SELECT DISTINCT session_id FROM observed_event LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    let state = AppState::new_for_tests(pool);
    let server = TestServer::new(wimcc::api::router(state)).unwrap();
    (server, sid)
}

async fn init_mcp_session(server: &TestServer) -> String {
    let r = server
        .post("/mcp")
        .content_type("application/json")
        .json(&json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "test", "version": "0"}
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
async fn resources_read_session_carries_redaction_annotation() {
    let (server, session_id) = make_server_with_session().await;
    let mcp_sid = init_mcp_session(&server).await;
    let (hk, hv) = sid_header(&mcp_sid);

    let r = server
        .post("/mcp")
        .content_type("application/json")
        .add_header(hk, hv)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "resources/read",
            "params": {
                "uri": format!("whats-in-my-cc://sessions/{session_id}")
            }
        }))
        .await;

    r.assert_status_ok();
    let body: Value = r.json();
    let contents = body["result"]["contents"]
        .as_array()
        .expect("result.contents must be an array");
    assert!(!contents.is_empty(), "contents must be non-empty");

    let ann = &contents[0]["annotations"];
    assert!(
        ann["redaction_policy"].is_object(),
        "annotations.redaction_policy must be an object; got: {ann}"
    );
    assert_eq!(
        ann["redaction_policy"]["applied"],
        Value::Bool(true),
        "redaction_policy.applied must be true"
    );
}
