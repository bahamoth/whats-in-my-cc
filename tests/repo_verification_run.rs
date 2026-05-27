//! Slice-11 — roundtrip test for `repo_verification_run`.
//! (Phase 2 regression lock.)

use sqlx::sqlite::SqlitePoolOptions;
use witmcc::db::{migrate, repo_verification_run};

#[tokio::test]
async fn roundtrip_with_null_optionals() {
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();

    let row = repo_verification_run::VerificationRunRow {
        verification_run_id: "vr_roundtrip".into(),
        schema_version: "verification_run.v1".into(),
        session_id: "sess_rt".into(),
        source: "bash".into(),
        command: "cargo test --lib".into(),
        command_kind: "test_suite_rust".into(),
        trigger_event_id: "ev_rt_001".into(),
        trigger_tool_use_id: None,
        status: "unknown".into(),
        started_at: "2026-05-27T11:00:00Z".into(),
        ended_at: None,
        exit_code: None,
        failure_summary: None,
        raw_event_id: "raw_rt_001".into(),
        parser_version: "verification_run@v1".into(),
    };
    repo_verification_run::insert(&pool, &row).await.unwrap();
    let fetched = repo_verification_run::get(&pool, "vr_roundtrip")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fetched.trigger_tool_use_id, None);
    assert_eq!(fetched.ended_at, None);
    assert_eq!(fetched.exit_code, None);
    assert_eq!(fetched.failure_summary, None);
    assert_eq!(fetched.status, "unknown");
}

#[tokio::test]
async fn list_empty_session_returns_empty_vec() {
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    let rows = repo_verification_run::list_session(&pool, "no_such_session")
        .await
        .unwrap();
    assert!(rows.is_empty());
}
