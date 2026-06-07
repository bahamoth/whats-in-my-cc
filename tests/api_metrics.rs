//! Plan 3a — integration tests for GET /v1/sessions/:id/metrics.
//!
//! Seeds events + signals, calls the endpoint, asserts the SessionMetrics
//! shape: correct counts/rates, no severity field.

use axum_test::TestServer;
use serde_json::Value;
use sqlx::sqlite::SqlitePoolOptions;
use wimcc::api::AppState;
use wimcc::db::{migrate, repo_observed, repo_raw, repo_runs, repo_signal};
use wimcc::db::repo_signal::SignalRow;
use wimcc::model::observed::{Actor, EventKind, ObservedEvent};

// ---------------------------------------------------------------------------
// Helpers (mirrors metrics_compute.rs seeders)
// ---------------------------------------------------------------------------

async fn test_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    pool
}

async fn seed_event(pool: &sqlx::SqlitePool, run_id: &str, session_id: &str, event_id: &str, kind: EventKind) {
    let raw_id = format!("raw_{event_id}");
    repo_raw::insert_dedup(pool, &repo_raw::NewRaw {
        raw_event_id: raw_id.clone(),
        ingest_run_id: run_id.into(),
        source_type: "claude_transcript".into(),
        source_uri: "/tmp/test.jsonl".into(),
        source_line_no: 0,
        source_byte_offset: 0,
        payload_sha256: format!("sha_{event_id}"),
        payload: b"{}".to_vec(),
        parse_error: None,
        captured_at: chrono::Utc::now(),
        redaction_state: "not_applicable".into(),
        redaction_manifest: None,
    }).await.unwrap();

    let e = ObservedEvent {
        event_id: event_id.into(),
        raw_event_id: raw_id,
        schema_version: "observed_event.v1".into(),
        session_id: session_id.into(),
        observed_at: chrono::Utc::now(),
        actor: Actor::Assistant,
        kind,
        parser_version: "test@v0".into(),
        ..Default::default()
    };
    repo_observed::insert(pool, &e).await.unwrap();
}

fn make_signal(session_id: &str, signal_id: &str, detector: &str) -> SignalRow {
    SignalRow {
        signal_id: signal_id.into(),
        schema_version: "signal.v1".into(),
        session_id: session_id.into(),
        detector: detector.into(),
        subkind: None,
        summary: format!("{detector} fired"),
        evidence_refs: "[]".into(),
        facts: "{}".into(),
        provenance: format!("{{\"detector\":\"{detector}@v1\"}}"),
        created_at: "2026-06-07T00:00:00Z".into(),
    }
}

fn build_server(pool: sqlx::SqlitePool) -> TestServer {
    let state = AppState::new_for_tests(pool);
    TestServer::new(wimcc::api::router(state)).unwrap()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn metrics_returns_200_with_correct_shape() {
    let pool = test_pool().await;
    let sid = "sess_api_metrics_1";
    let run_id = repo_runs::start(&pool).await.unwrap();

    seed_event(&pool, &run_id, sid, "tc_a1", EventKind::ToolCall).await;
    seed_event(&pool, &run_id, sid, "tc_a2", EventKind::ToolCall).await;
    repo_signal::insert(&pool, &make_signal(sid, "sig_tf_a1", "tool_failure")).await.unwrap();

    let server = build_server(pool);
    let r = server.get(&format!("/v1/sessions/{sid}/metrics")).await;
    r.assert_status_ok();

    let body: Value = r.json();
    let data = &body["data"];

    assert_eq!(data["session_id"].as_str().unwrap(), sid);
    assert_eq!(data["tool_call_total"].as_i64().unwrap(), 2);
    assert_eq!(data["tool_failure_count"].as_i64().unwrap(), 1);

    let rate = data["tool_failure_rate"].as_f64().unwrap();
    assert!((rate - 0.5).abs() < 1e-9, "tool_failure_rate={rate}");

    assert!(data.get("severity").is_none(), "metrics must NOT have a severity field");
}

#[tokio::test]
async fn metrics_detector_firing_map_present() {
    let pool = test_pool().await;
    let sid = "sess_api_metrics_2";
    let run_id = repo_runs::start(&pool).await.unwrap();

    seed_event(&pool, &run_id, sid, "tc_b1", EventKind::ToolCall).await;
    repo_signal::insert(&pool, &make_signal(sid, "sig_cb_b1", "context_bloat")).await.unwrap();
    repo_signal::insert(&pool, &make_signal(sid, "sig_cb_b2", "context_bloat")).await.unwrap();
    repo_signal::insert(&pool, &make_signal(sid, "sig_tf_b1", "tool_failure")).await.unwrap();

    let server = build_server(pool);
    let r = server.get(&format!("/v1/sessions/{sid}/metrics")).await;
    r.assert_status_ok();

    let body: Value = r.json();
    let firing = &body["data"]["detector_firing"];
    assert!(firing.is_object(), "detector_firing must be an object");
    assert_eq!(firing["context_bloat"].as_i64().unwrap(), 2);
    assert_eq!(firing["tool_failure"].as_i64().unwrap(), 1);
}

#[tokio::test]
async fn metrics_empty_session_all_zeros() {
    let pool = test_pool().await;
    let sid = "sess_api_metrics_empty";

    let server = build_server(pool);
    let r = server.get(&format!("/v1/sessions/{sid}/metrics")).await;
    r.assert_status_ok();

    let body: Value = r.json();
    let data = &body["data"];
    assert_eq!(data["tool_call_total"].as_i64().unwrap(), 0);
    assert_eq!(data["tool_failure_rate"].as_f64().unwrap(), 0.0);
    assert!(data["detector_firing"].is_object());
}
