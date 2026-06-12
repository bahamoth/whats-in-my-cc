//! GET /v1/sessions/:id/fingerprint — envelope + 결정론 관측 필드 (Task 4).

use serde_json::{json, Value};
use sqlx::sqlite::SqlitePoolOptions;
use wimcc::api::AppState;
use wimcc::db::{migrate, repo_observed, repo_raw, repo_runs};
use wimcc::model::observed::{Actor, EventKind, ObservedEvent};

async fn test_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    pool
}

async fn seed_assistant(pool: &sqlx::SqlitePool, run_id: &str, sid: &str, eid: &str, model: &str) {
    let raw_id = format!("raw_{eid}");
    repo_raw::insert_dedup(
        pool,
        &repo_raw::NewRaw {
            raw_event_id: raw_id.clone(),
            ingest_run_id: run_id.into(),
            source_type: "claude_transcript".into(),
            source_uri: "/tmp/fpapi.jsonl".into(),
            source_line_no: 0,
            source_byte_offset: 0,
            payload_sha256: format!("sha_{eid}"),
            payload: b"{}".to_vec(),
            parse_error: None,
            captured_at: chrono::Utc::now(),
            redaction_state: "not_applicable".into(),
            redaction_manifest: None,
        },
    )
    .await
    .unwrap();
    let e = ObservedEvent {
        event_id: eid.into(),
        raw_event_id: raw_id,
        schema_version: "observed_event.v1".into(),
        session_id: sid.into(),
        observed_at: chrono::Utc::now(),
        actor: Actor::Assistant,
        kind: EventKind::AssistantMessage,
        payload: json!({"model": model}),
        cc_version: Some("2.1.0".into()),
        parser_version: "test@v0".into(),
        ..Default::default()
    };
    repo_observed::insert(pool, &e).await.unwrap();
}

#[tokio::test]
async fn fingerprint_endpoint_returns_envelope_with_models() {
    let pool = test_pool().await;
    let run = repo_runs::start(&pool).await.unwrap();
    seed_assistant(&pool, &run, "sess_fp_api", "e1", "claude-opus-4-7").await;
    let server =
        axum_test::TestServer::new(wimcc::api::router(AppState::new_for_tests(pool))).unwrap();
    let r = server.get("/v1/sessions/sess_fp_api/fingerprint").await;
    r.assert_status_ok();
    let body: Value = r.json();
    assert!(body["meta"]["schema_version"].is_string());
    let data = &body["data"];
    assert_eq!(data["session_id"], "sess_fp_api");
    assert_eq!(data["models"][0], "claude-opus-4-7");
    assert_eq!(data["cc_versions"][0], "2.1.0");
    assert!(data["claude_md"].as_array().unwrap().is_empty());
    assert!(data["instruction_sha256"].is_null());
}

#[tokio::test]
async fn fingerprint_endpoint_empty_session_returns_empty_observation() {
    let pool = test_pool().await;
    let server =
        axum_test::TestServer::new(wimcc::api::router(AppState::new_for_tests(pool))).unwrap();
    let r = server.get("/v1/sessions/sess_fp_void/fingerprint").await;
    r.assert_status_ok();
    let body: Value = r.json();
    assert!(body["data"]["models"].as_array().unwrap().is_empty());
}
