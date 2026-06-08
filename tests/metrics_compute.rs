//! Plan 3a — unit tests for `compute_session_metrics`.
//!
//! Verifies deterministic aggregation over events + signals.
//! Facts/counts only — no window-fixed rates (spec F1), no severity/judgment fields.

use sqlx::sqlite::SqlitePoolOptions;
use wimcc::db::repo_signal::SignalRow;
use wimcc::db::repo_verification_run::VerificationRunRow;
use wimcc::db::{migrate, repo_observed, repo_raw, repo_runs, repo_signal, repo_verification_run};
use wimcc::insight::metrics::compute_session_metrics;
use wimcc::model::observed::{Actor, EventKind, ObservedEvent};

// ---------------------------------------------------------------------------
// Helpers
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

/// Insert a minimal `ingest_run` + `raw_event` row so `observed_event` FK is
/// satisfied, then insert the event. One raw row is reused per (pool, event_id)
/// by using the event_id as the raw_event_id; duplicates are ignored via
/// `insert_dedup`.
async fn seed_event(
    pool: &sqlx::SqlitePool,
    run_id: &str,
    session_id: &str,
    event_id: &str,
    kind: EventKind,
) {
    let raw_id = format!("raw_{event_id}");
    repo_raw::insert_dedup(
        pool,
        &repo_raw::NewRaw {
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
        },
    )
    .await
    .unwrap();

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

fn make_vrun(session_id: &str, id: &str, status: &str) -> VerificationRunRow {
    VerificationRunRow {
        verification_run_id: id.into(),
        schema_version: "verification_run.v1".into(),
        session_id: session_id.into(),
        source: "bash".into(),
        command: "cargo test".into(),
        command_kind: "test_suite_rust".into(),
        trigger_event_id: format!("ev_{id}"),
        trigger_tool_use_id: None,
        status: status.into(),
        status_provenance: Some("measured".into()),
        detection_basis: "known_tool".into(),
        status_basis: "exit".into(),
        started_at: "2026-06-07T00:00:01Z".into(),
        ended_at: Some("2026-06-07T00:00:02Z".into()),
        exit_code: Some(if status == "passed" { 0 } else { 1 }),
        failure_summary: None,
        raw_event_id: format!("raw_vr_{id}"),
        parser_version: "verification_run@v1".into(),
    }
}

// ---------------------------------------------------------------------------
// Tool call total + tool failure count + detector_firing
// ---------------------------------------------------------------------------

#[tokio::test]
async fn aggregates_tool_failure_and_detector_firing() {
    let pool = test_pool().await;
    let sid = "s_metrics_1";
    let run_id = repo_runs::start(&pool).await.unwrap();

    seed_event(&pool, &run_id, sid, "tc1", EventKind::ToolCall).await;
    seed_event(&pool, &run_id, sid, "tc2", EventKind::ToolCall).await;
    repo_signal::insert(&pool, &make_signal(sid, "sig_tf1", "tool_failure"))
        .await
        .unwrap();

    let m = compute_session_metrics(&pool, sid).await.unwrap();
    assert_eq!(m.tool_call_total, 2);
    assert_eq!(m.tool_failure_count, 1);
    assert_eq!(m.detector_firing.get("tool_failure"), Some(&1));
    // no severity field — compile-time: SessionMetrics has no severity field
}

// ---------------------------------------------------------------------------
// Zero tool_calls — counts stay 0, no divide-by-zero (rate is the consumer's job)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn tool_failure_count_when_no_tool_calls() {
    let pool = test_pool().await;
    let sid = "s_metrics_zero";
    let run_id = repo_runs::start(&pool).await.unwrap();

    seed_event(&pool, &run_id, sid, "um1", EventKind::UserMessage).await;
    repo_signal::insert(&pool, &make_signal(sid, "sig_tf2", "tool_failure"))
        .await
        .unwrap();

    let m = compute_session_metrics(&pool, sid).await.unwrap();
    assert_eq!(m.tool_call_total, 0);
    assert_eq!(m.tool_failure_count, 1);
}

// ---------------------------------------------------------------------------
// Multiple detectors — detector_firing map has both
// ---------------------------------------------------------------------------

#[tokio::test]
async fn multiple_detectors_all_appear_in_map() {
    let pool = test_pool().await;
    let sid = "s_metrics_multi";
    let run_id = repo_runs::start(&pool).await.unwrap();

    seed_event(&pool, &run_id, sid, "tc_m1", EventKind::ToolCall).await;
    repo_signal::insert(&pool, &make_signal(sid, "sig_tf_m1", "tool_failure"))
        .await
        .unwrap();
    repo_signal::insert(&pool, &make_signal(sid, "sig_cb_m1", "context_bloat"))
        .await
        .unwrap();
    repo_signal::insert(&pool, &make_signal(sid, "sig_cb_m2", "context_bloat"))
        .await
        .unwrap();

    let m = compute_session_metrics(&pool, sid).await.unwrap();
    assert_eq!(m.detector_firing.get("tool_failure"), Some(&1));
    assert_eq!(m.detector_firing.get("context_bloat"), Some(&2));
    assert_eq!(m.context_bloat_count, 2);
    assert_eq!(m.tool_failure_count, 1);
}

// ---------------------------------------------------------------------------
// Verification runs — passed/failed counts
// ---------------------------------------------------------------------------

#[tokio::test]
async fn verification_counts_computed_correctly() {
    let pool = test_pool().await;
    let sid = "s_metrics_vr";
    let run_id = repo_runs::start(&pool).await.unwrap();

    seed_event(&pool, &run_id, sid, "tc_vr1", EventKind::ToolCall).await;
    repo_verification_run::insert(&pool, &make_vrun(sid, "vr1", "passed"))
        .await
        .unwrap();
    repo_verification_run::insert(&pool, &make_vrun(sid, "vr2", "failed"))
        .await
        .unwrap();
    repo_verification_run::insert(&pool, &make_vrun(sid, "vr3", "passed"))
        .await
        .unwrap();

    let m = compute_session_metrics(&pool, sid).await.unwrap();
    assert_eq!(m.verification_total, 3);
    assert_eq!(m.verification_passed, 2);
    assert_eq!(m.verification_failed, 1);
    assert_eq!(m.verification_unknown, 0);
    // rate는 소비자가 passed / (passed + failed) 로 직접 계산한다.
}

// ---------------------------------------------------------------------------
// Empty session — all zeros, no panic
// ---------------------------------------------------------------------------

#[tokio::test]
async fn empty_session_returns_all_zeros() {
    let pool = test_pool().await;
    let sid = "s_metrics_empty";

    let m = compute_session_metrics(&pool, sid).await.unwrap();
    assert_eq!(m.tool_call_total, 0);
    assert_eq!(m.tool_failure_count, 0);
    assert_eq!(m.verification_total, 0);
    assert_eq!(m.verification_passed, 0);
    assert_eq!(m.verification_failed, 0);
    assert_eq!(m.verification_unknown, 0);
    assert_eq!(m.context_bloat_count, 0);
    assert!(m.detector_firing.is_empty());
}

// ---------------------------------------------------------------------------
// Verification runs — passed/failed/unknown separated (spec F1)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn metrics_separates_verification_unknown_from_measured() {
    let pool = test_pool().await;
    let sid = "s_metrics_vr_sep";
    let run_id = repo_runs::start(&pool).await.unwrap();

    seed_event(&pool, &run_id, sid, "tc_sep1", EventKind::ToolCall).await;

    // 6 runs: passed 1, failed 2, unknown 3
    repo_verification_run::insert(&pool, &make_vrun(sid, "vrs_p1", "passed"))
        .await
        .unwrap();
    repo_verification_run::insert(&pool, &make_vrun(sid, "vrs_f1", "failed"))
        .await
        .unwrap();
    repo_verification_run::insert(&pool, &make_vrun(sid, "vrs_f2", "failed"))
        .await
        .unwrap();
    repo_verification_run::insert(&pool, &make_vrun(sid, "vrs_u1", "unknown"))
        .await
        .unwrap();
    repo_verification_run::insert(&pool, &make_vrun(sid, "vrs_u2", "unknown"))
        .await
        .unwrap();
    repo_verification_run::insert(&pool, &make_vrun(sid, "vrs_u3", "unknown"))
        .await
        .unwrap();

    let m = compute_session_metrics(&pool, sid).await.unwrap();
    assert_eq!(m.verification_total, 6);
    assert_eq!(m.verification_passed, 1);
    assert_eq!(m.verification_failed, 2);
    assert_eq!(m.verification_unknown, 3);
    // measured = passed + failed = 3; unknown은 분모에서 분리되어 별도 노출.
}
