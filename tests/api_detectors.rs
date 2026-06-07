//! Plan 4 — HTTP route tests for GET /v1/detectors (manifest catalog).
//!
//! The endpoint must return all 4 registered detectors with their manifests.
//! Each manifest must include id, intent, inputs, rule, output, config_keys,
//! and rationale — no judgment fields.

use axum_test::TestServer;
use serde_json::Value;
use sqlx::sqlite::SqlitePoolOptions;
use wimcc::api::AppState;
use wimcc::db::migrate;

async fn build_server() -> TestServer {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    let state = AppState::new_for_tests(pool);
    TestServer::new(wimcc::api::router(state)).unwrap()
}

#[tokio::test]
async fn detectors_returns_200() {
    let server = build_server().await;
    let r = server.get("/v1/detectors").await;
    r.assert_status_ok();
}

#[tokio::test]
async fn detectors_returns_four_manifests() {
    let server = build_server().await;
    let r = server.get("/v1/detectors").await;
    r.assert_status_ok();
    let body: Value = r.json();
    let data = body["data"].as_array().expect("response must have a 'data' array");
    assert_eq!(
        data.len(),
        4,
        "expected exactly 4 detector manifests, got {n}. data: {data:?}",
        n = data.len(),
    );
}

#[tokio::test]
async fn detectors_includes_expected_ids() {
    let server = build_server().await;
    let r = server.get("/v1/detectors").await;
    r.assert_status_ok();
    let body: Value = r.json();
    let data = body["data"].as_array().unwrap();
    let ids: Vec<&str> = data.iter().map(|m| m["id"].as_str().unwrap()).collect();

    let expected = [
        "tool_failure",
        "risky_action",
        "context_bloat",
        "final_state_mismatch",
    ];
    for exp in &expected {
        assert!(
            ids.contains(exp),
            "detectors missing '{exp}'; got: {ids:?}",
        );
    }
}

#[tokio::test]
async fn detectors_each_manifest_has_required_fields() {
    let server = build_server().await;
    let r = server.get("/v1/detectors").await;
    r.assert_status_ok();
    let body: Value = r.json();
    let data = body["data"].as_array().unwrap();

    for m in data {
        let id = m["id"].as_str().unwrap_or("<missing id>");
        assert!(m["id"].is_string(), "[{id}] id must be a string");
        assert!(
            m["intent"].is_string() && !m["intent"].as_str().unwrap().is_empty(),
            "[{id}] intent must be a non-empty string",
        );
        assert!(m["inputs"].is_array(), "[{id}] inputs must be an array");
        assert!(
            m["rule"].is_string() && !m["rule"].as_str().unwrap().is_empty(),
            "[{id}] rule must be a non-empty string",
        );
        assert!(
            m["output"].is_string() && !m["output"].as_str().unwrap().is_empty(),
            "[{id}] output must be a non-empty string",
        );
        assert!(
            m["config_keys"].is_array(),
            "[{id}] config_keys must be an array",
        );
        assert!(
            m["rationale"].is_string() && !m["rationale"].as_str().unwrap().is_empty(),
            "[{id}] rationale must be a non-empty string",
        );
    }
}

#[tokio::test]
async fn detectors_tool_failure_has_is_error_input() {
    let server = build_server().await;
    let r = server.get("/v1/detectors").await;
    r.assert_status_ok();
    let body: Value = r.json();
    let data = body["data"].as_array().unwrap();
    let tf = data
        .iter()
        .find(|m| m["id"].as_str() == Some("tool_failure"))
        .expect("tool_failure manifest must be present");

    let inputs: Vec<&str> = tf["inputs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(
        inputs.iter().any(|i| i.contains("is_error")),
        "tool_failure.inputs must contain 'is_error'; got: {inputs:?}",
    );
}
