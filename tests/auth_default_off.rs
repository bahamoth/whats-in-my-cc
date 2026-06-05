//! Post-slice-19 — `--auth off` default for single-user dev.
//!
//! Three regression locks (already green today thanks to middleware's empty-token
//! bypass) + one RED test that fails until `--auth` flag is wired into the
//! CLI surface with `default: off`.

use axum_test::TestServer;
use serde_json::json;

/// Build a test server with auth DISABLED (token = empty string).
/// Matches the production path produced by `wimcc serve --auth off`.
async fn build_anonymous_test_server() -> TestServer {
    let pool = wimcc::db::connect(":memory:").await.unwrap();
    wimcc::db::migrate(&pool).await.unwrap();

    // new_for_tests() leaves token = "" by default — this is the auth-off shape.
    let state = wimcc::api::AppState::new_for_tests(pool);
    assert!(state.token.is_empty(), "auth-off invariant: token must be empty");

    let app = wimcc::api::router(state);
    TestServer::new(app).unwrap()
}

#[tokio::test]
async fn empty_token_allows_anonymous_v1_health() {
    let server = build_anonymous_test_server().await;
    let r = server.get("/v1/health").await;
    r.assert_status_ok();
    let body: serde_json::Value = r.json();
    assert_eq!(
        body["security"]["auth_required"],
        json!(false),
        "auth_required must mirror token emptiness"
    );
}

#[tokio::test]
async fn empty_token_allows_anonymous_v1_sessions() {
    let server = build_anonymous_test_server().await;
    let r = server.get("/v1/sessions").await;
    r.assert_status_ok();
}

#[tokio::test]
async fn empty_token_allows_anonymous_mcp_initialize() {
    let server = build_anonymous_test_server().await;
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
    r.assert_status_ok();
}

/// RED — until `--auth` flag with `default: off` is added to `serve`.
#[test]
fn serve_help_lists_auth_flag_with_default_off() {
    let out = assert_cmd::Command::cargo_bin("wimcc")
        .unwrap()
        .args(["serve", "--help"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("--auth"),
        "serve --help must mention --auth flag; got:\n{stdout}"
    );
    assert!(
        stdout.contains("[default: off]"),
        "--auth must default to off; got:\n{stdout}"
    );
}
