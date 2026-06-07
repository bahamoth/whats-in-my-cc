//! Plan 3a — unit tests for `compute_session_metrics`.
//!
//! Verifies deterministic aggregation over events + signals.
//! Facts/counts/ratios only — no severity/judgment fields.

use sqlx::sqlite::SqlitePoolOptions;
use wimcc::db::{migrate, repo_observed, repo_raw, repo_runs, repo_signal, repo_usage_facet, repo_verification_run};
use wimcc::db::repo_signal::SignalRow;
use wimcc::db::repo_usage_facet::UsageFacetRow;
use wimcc::db::repo_verification_run::VerificationRunRow;
use wimcc::model::observed::{Actor, EventKind, ObservedEvent};
use wimcc::insight::metrics::compute_session_metrics;

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

fn make_usage(session_id: &str, raw_id: &str, input: i64, cc: i64, cr: i64) -> UsageFacetRow {
    UsageFacetRow {
        raw_event_id: raw_id.into(),
        schema_version: "usage_facet.v1".into(),
        session_id: session_id.into(),
        model: Some("claude-opus-4-8".into()),
        input_tokens: input,
        cache_creation_input_tokens: cc,
        cache_read_input_tokens: cr,
        output_tokens: 100,
        observed_at: "2026-06-07T00:00:00Z".into(),
        parser_version: "usage_facet@v1".into(),
    }
}

// ---------------------------------------------------------------------------
// Tool call total + tool failure count + rate + detector_firing
// ---------------------------------------------------------------------------

#[tokio::test]
async fn aggregates_tool_failure_and_detector_firing() {
    let pool = test_pool().await;
    let sid = "s_metrics_1";
    let run_id = repo_runs::start(&pool).await.unwrap();

    seed_event(&pool, &run_id, sid, "tc1", EventKind::ToolCall).await;
    seed_event(&pool, &run_id, sid, "tc2", EventKind::ToolCall).await;
    repo_signal::insert(&pool, &make_signal(sid, "sig_tf1", "tool_failure")).await.unwrap();

    let m = compute_session_metrics(&pool, sid).await.unwrap();
    assert_eq!(m.tool_call_total, 2);
    assert_eq!(m.tool_failure_count, 1);
    assert!((m.tool_failure_rate - 0.5).abs() < 1e-9, "rate=0.5 expected, got {}", m.tool_failure_rate);
    assert_eq!(m.detector_firing.get("tool_failure"), Some(&1));
    // no severity field — compile-time: SessionMetrics has no severity field
}

// ---------------------------------------------------------------------------
// Zero tool_calls — rate should be 0.0, not NaN/panic
// ---------------------------------------------------------------------------

#[tokio::test]
async fn rate_is_zero_when_no_tool_calls() {
    let pool = test_pool().await;
    let sid = "s_metrics_zero";
    let run_id = repo_runs::start(&pool).await.unwrap();

    seed_event(&pool, &run_id, sid, "um1", EventKind::UserMessage).await;
    repo_signal::insert(&pool, &make_signal(sid, "sig_tf2", "tool_failure")).await.unwrap();

    let m = compute_session_metrics(&pool, sid).await.unwrap();
    assert_eq!(m.tool_call_total, 0);
    assert_eq!(m.tool_failure_count, 1);
    assert_eq!(m.tool_failure_rate, 0.0, "rate must be 0 when denominator is 0");
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
    repo_signal::insert(&pool, &make_signal(sid, "sig_tf_m1", "tool_failure")).await.unwrap();
    repo_signal::insert(&pool, &make_signal(sid, "sig_cb_m1", "context_bloat")).await.unwrap();
    repo_signal::insert(&pool, &make_signal(sid, "sig_cb_m2", "context_bloat")).await.unwrap();

    let m = compute_session_metrics(&pool, sid).await.unwrap();
    assert_eq!(m.detector_firing.get("tool_failure"), Some(&1));
    assert_eq!(m.detector_firing.get("context_bloat"), Some(&2));
    assert_eq!(m.context_bloat_count, 2);
    assert_eq!(m.tool_failure_count, 1);
}

// ---------------------------------------------------------------------------
// Verification runs — pass rate
// ---------------------------------------------------------------------------

#[tokio::test]
async fn verification_pass_rate_computed_correctly() {
    let pool = test_pool().await;
    let sid = "s_metrics_vr";
    let run_id = repo_runs::start(&pool).await.unwrap();

    seed_event(&pool, &run_id, sid, "tc_vr1", EventKind::ToolCall).await;
    repo_verification_run::insert(&pool, &make_vrun(sid, "vr1", "passed")).await.unwrap();
    repo_verification_run::insert(&pool, &make_vrun(sid, "vr2", "failed")).await.unwrap();
    repo_verification_run::insert(&pool, &make_vrun(sid, "vr3", "passed")).await.unwrap();

    let m = compute_session_metrics(&pool, sid).await.unwrap();
    assert_eq!(m.verification_total, 3);
    assert_eq!(m.verification_passed, 2);
    assert!((m.verification_pass_rate - 2.0 / 3.0).abs() < 1e-9, "pass_rate={}", m.verification_pass_rate);
}

// ---------------------------------------------------------------------------
// Cache hit ratio from usage_facet (no FK to observed_event)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn cache_hit_ratio_computed_from_usage_facet() {
    let pool = test_pool().await;
    let sid = "s_metrics_cache";

    // input=10, cc=0, cr=90 → denom=100, ratio=0.9
    repo_usage_facet::insert(&pool, &make_usage(sid, "raw_c1", 10, 0, 90)).await.unwrap();

    let m = compute_session_metrics(&pool, sid).await.unwrap();
    assert!((m.cache_hit_ratio - 0.9).abs() < 1e-9, "cache_hit_ratio={}", m.cache_hit_ratio);
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
    assert_eq!(m.tool_failure_rate, 0.0);
    assert_eq!(m.verification_total, 0);
    assert_eq!(m.verification_passed, 0);
    assert_eq!(m.verification_pass_rate, 0.0);
    assert_eq!(m.context_bloat_count, 0);
    assert_eq!(m.cache_hit_ratio, 0.0);
    assert!(m.detector_firing.is_empty());
}
