//! Plan 1 — HTTP route tests for /v1/sessions/:id/signals and /v1/signals/:id.
//!
//! Signals are deterministic facts: NO severity/confidence/status fields.
//! These tests pin the signal API shape and assert those judgment fields are
//! absent from the response.

use axum_test::TestServer;
use serde_json::Value;
use sqlx::sqlite::SqlitePoolOptions;
use wimcc::api::AppState;
use wimcc::db::{migrate, repo_signal};

fn sample_row(session_id: &str, signal_id: &str) -> repo_signal::SignalRow {
    repo_signal::SignalRow {
        signal_id: signal_id.into(),
        schema_version: "signal.v1".into(),
        session_id: session_id.into(),
        detector: "tool_failure".into(),
        subkind: None,
        summary: "Tool Bash returned is_error=true (retried=false).".into(),
        evidence_refs: r#"["ev_1","ev_2"]"#.into(),
        facts: r#"{"is_error":true,"tool_name":"Bash"}"#.into(),
        provenance: r#"{"detector":"tool_failure@v1","version":"L1"}"#.into(),
        created_at: "2026-06-07T00:00:00Z".into(),
    }
}

async fn pool_with_seeded_signals() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    repo_signal::insert(&pool, &sample_row("sess_demo", "sig_demo_001"))
        .await
        .unwrap();
    pool
}

fn build_server(pool: sqlx::SqlitePool) -> TestServer {
    let state = AppState::new_for_tests(pool);
    TestServer::new(wimcc::api::router(state)).unwrap()
}

#[tokio::test]
async fn session_signals_returns_inserted() {
    let server = build_server(pool_with_seeded_signals().await);
    let r = server.get("/v1/sessions/sess_demo/signals").await;
    r.assert_status_ok();
    let body: Value = r.json();
    let data = body["data"].as_array().unwrap();
    assert_eq!(data.len(), 1, "expected exactly 1 signal, got {}", data.len());
    assert_eq!(data[0]["detector"].as_str().unwrap(), "tool_failure");
    // evidence_refs and facts are parsed JSON, not strings.
    assert!(data[0]["evidence_refs"].is_array());
    assert!(data[0]["facts"].is_object());
    assert_eq!(data[0]["facts"]["tool_name"].as_str().unwrap(), "Bash");
}

#[tokio::test]
async fn signal_response_has_no_judgment_fields() {
    let server = build_server(pool_with_seeded_signals().await);
    let r = server.get("/v1/sessions/sess_demo/signals").await;
    r.assert_status_ok();
    let body: Value = r.json();
    let signal = &body["data"][0];
    // Signals carry facts only — no severity/confidence/status judgments.
    assert!(signal.get("severity").is_none(), "signal must NOT have a severity field");
    assert!(signal.get("confidence").is_none(), "signal must NOT have a confidence field");
    assert!(signal.get("status").is_none(), "signal must NOT have a status field");
}

#[tokio::test]
async fn signal_detail_returns_signal() {
    let server = build_server(pool_with_seeded_signals().await);
    let r = server.get("/v1/signals/sig_demo_001").await;
    r.assert_status_ok();
    let body: Value = r.json();
    assert_eq!(body["data"]["signal_id"].as_str().unwrap(), "sig_demo_001");
    assert_eq!(body["data"]["detector"].as_str().unwrap(), "tool_failure");
    assert!(body["data"].get("severity").is_none());
}

#[tokio::test]
async fn signal_detail_404_for_unknown_id() {
    let server = build_server(pool_with_seeded_signals().await);
    let r = server.get("/v1/signals/sig_does_not_exist").await;
    r.assert_status(axum::http::StatusCode::NOT_FOUND);
}
