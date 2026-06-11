//! Slice-11 — API endpoint tests for verification-runs.
//! (TDD red — Phase 1 commit 1.)
//!
//! Tests:
//! - `GET /v1/sessions/:id/verification-runs` — list
//! - `GET /v1/verification-runs/:id` — detail
//! - Empty session case (no runs yet)

use axum_test::TestServer;
use serde_json::Value;
use sqlx::sqlite::SqlitePoolOptions;
use wimcc::api::{router, AppState};
use wimcc::db::migrate;

async fn empty_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    pool
}

/// Inserts 2 verification_run rows directly into the DB for session "sess_t1".
/// Row IDs: "vr_t1_001" and "vr_t1_002".
async fn seeded_pool() -> sqlx::SqlitePool {
    let pool = empty_pool().await;
    sqlx::query(
        "INSERT INTO verification_run(
            verification_run_id, schema_version, session_id, source, command,
            command_kind, trigger_event_id, trigger_tool_use_id, status,
            started_at, raw_event_id, parser_version)
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind("vr_t1_001")
    .bind("verification_run.v1")
    .bind("sess_t1")
    .bind("bash")
    .bind("cargo test")
    .bind("test_suite_rust")
    .bind("ev_tool_result_001")
    .bind("toolu_001")
    .bind("passed")
    .bind("2026-05-27T10:00:00Z")
    .bind("raw_001")
    .bind("verification_run@v1")
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO verification_run(
            verification_run_id, schema_version, session_id, source, command,
            command_kind, trigger_event_id, status,
            started_at, raw_event_id, parser_version)
         VALUES (?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind("vr_t1_002")
    .bind("verification_run.v1")
    .bind("sess_t1")
    .bind("bash")
    .bind("cargo build --release")
    .bind("build")
    .bind("ev_tool_result_002")
    .bind("failed")
    .bind("2026-05-27T10:01:00Z")
    .bind("raw_002")
    .bind("verification_run@v1")
    .execute(&pool)
    .await
    .unwrap();

    pool
}

#[tokio::test]
async fn list_endpoint_returns_runs_for_session() {
    let pool = seeded_pool().await;
    let app = router(AppState::new_for_tests(pool));
    let server = TestServer::new(app).unwrap();

    let resp = server.get("/v1/sessions/sess_t1/verification-runs").await;
    resp.assert_status_ok();
    let body: Value = resp.json();
    let data = body["data"].as_array().unwrap();
    assert_eq!(
        data.len(),
        2,
        "expected 2 runs for sess_t1, got {}",
        data.len()
    );

    // Verify shape of first item
    let first = &data[0];
    assert!(
        first["verification_run_id"].is_string(),
        "verification_run_id must be a string"
    );
    assert!(
        first["covered_diff_hunk_ids"].is_array(),
        "covered_diff_hunk_ids must be a JSON array"
    );
    assert!(
        first["schema_version"].is_string(),
        "schema_version must be present"
    );
    assert!(
        first["detection_basis"].is_string(),
        "detection_basis must be present in DTO"
    );
    assert!(
        first["status_basis"].is_string(),
        "status_basis must be present in DTO"
    );
}

#[tokio::test]
async fn list_endpoint_returns_empty_for_unknown_session() {
    let pool = empty_pool().await;
    let app = router(AppState::new_for_tests(pool));
    let server = TestServer::new(app).unwrap();

    let resp = server
        .get("/v1/sessions/unknown_session_xyz/verification-runs")
        .await;
    resp.assert_status_ok();
    let body: Value = resp.json();
    let data = body["data"].as_array().unwrap();
    assert_eq!(data.len(), 0, "unknown session must return empty array");
}

#[tokio::test]
async fn detail_endpoint_returns_single_run() {
    let pool = seeded_pool().await;
    let app = router(AppState::new_for_tests(pool));
    let server = TestServer::new(app).unwrap();

    let resp = server.get("/v1/verification-runs/vr_t1_001").await;
    resp.assert_status_ok();
    let body: Value = resp.json();
    assert_eq!(
        body["data"]["verification_run_id"].as_str(),
        Some("vr_t1_001"),
        "expected vr_t1_001 in response"
    );
    assert!(
        body["data"]["covered_diff_hunk_ids"].is_array(),
        "covered_diff_hunk_ids must be a JSON array"
    );
}

#[tokio::test]
async fn detail_endpoint_returns_404_for_unknown_id() {
    let pool = empty_pool().await;
    let app = router(AppState::new_for_tests(pool));
    let server = TestServer::new(app).unwrap();

    let resp = server.get("/v1/verification-runs/vr_nonexistent").await;
    resp.assert_status(axum::http::StatusCode::NOT_FOUND);
}
