//! Plan 3a — unit tests for `compute_session_metrics`.
//!
//! Verifies deterministic aggregation over events + signals.
//! Facts/counts only — no window-fixed rates (spec F1), no severity/judgment fields.

use sqlx::sqlite::SqlitePoolOptions;
use wimcc::db::repo_signal::SignalRow;
use wimcc::db::repo_usage_facet;
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
    assert_eq!(m.turn_duration_count, 0);
    assert_eq!(m.turn_duration_ms_total, 0);
    assert_eq!(m.api_error_count, 0);
    assert_eq!(m.compact_boundary_count, 0);
    assert_eq!(m.tool_result_truncated_count, 0);
    assert_eq!(m.user_interruption_count, 0);
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
    // not_executed: disposition(거부/차단/취소/백그라운드) — 명령 미실행. unknown과
    // 별개 축이라 unknown 카운트를 부풀리지 않는다 (2026-06-23 review).
    repo_verification_run::insert(&pool, &make_vrun(sid, "vrs_ne1", "not_executed"))
        .await
        .unwrap();

    let m = compute_session_metrics(&pool, sid).await.unwrap();
    assert_eq!(m.verification_total, 7);
    assert_eq!(m.verification_passed, 1);
    assert_eq!(m.verification_failed, 2);
    assert_eq!(m.verification_unknown, 3);
    assert_eq!(m.verification_not_executed, 1);
    // measured = passed + failed = 3; unknown(실행됐으나 결과 미상)과
    // not_executed(미실행)는 각각 별도 축으로 분리 노출.
}

/// payload를 지정해 tool_result 이벤트를 시드한다 (disposition 카운트용).
async fn seed_tool_result_with_content(
    pool: &sqlx::SqlitePool,
    run_id: &str,
    session_id: &str,
    event_id: &str,
    content: &str,
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
        actor: Actor::Tool,
        kind: EventKind::ToolResult,
        tool_use_id: Some(format!("toolu_{event_id}")),
        parser_version: "test@v0".into(),
        payload: serde_json::json!({
            "tool_result": {"tool_use_id": format!("toolu_{event_id}"), "content": content}
        }),
        ..Default::default()
    };
    repo_observed::insert(pool, &e).await.unwrap();
}

#[tokio::test]
async fn disposition_counts_from_tool_result_markers() {
    // 마커 문구는 disposition_v01.jsonl 실 payload와 동일 형태
    // (classify_disposition 단위/통합 테스트에서 real fixture로 잠김).
    let pool = test_pool().await;
    let run_id = repo_runs::start(&pool).await.unwrap();

    seed_tool_result_with_content(
        &pool,
        &run_id,
        "sess_d",
        "e1",
        "The user doesn't want to proceed with this tool use. The tool use was rejected.",
    )
    .await;
    seed_tool_result_with_content(
        &pool,
        &run_id,
        "sess_d",
        "e2",
        "Hook PreToolUse:Bash denied this tool",
    )
    .await;
    seed_tool_result_with_content(
        &pool,
        &run_id,
        "sess_d",
        "e3",
        "<tool_use_error>Cancelled: parallel tool call Bash(x)</tool_use_error>",
    )
    .await;
    seed_tool_result_with_content(
        &pool,
        &run_id,
        "sess_d",
        "e4",
        "Command running in background with ID: abc123. Output is being written to: /tmp/t.output.",
    )
    .await;
    // 일반 출력 + 일반 tool_use_error(실행 실패)는 disposition 카운트에 포함되지 않는다.
    seed_tool_result_with_content(&pool, &run_id, "sess_d", "e5", "test result: ok. 1 passed")
        .await;
    seed_tool_result_with_content(
        &pool,
        &run_id,
        "sess_d",
        "e6",
        "<tool_use_error>File has been modified since read</tool_use_error>",
    )
    .await;

    let m = compute_session_metrics(&pool, "sess_d").await.unwrap();
    assert_eq!(m.tool_user_rejected, 1);
    assert_eq!(m.tool_policy_denied, 1);
    assert_eq!(m.tool_cancelled, 1);
    assert_eq!(m.tool_backgrounded, 1);
}

// ---------------------------------------------------------------------------
// Session facts — system 레코드 카운트 + 마커 카운트 (real 형태는
// session_facts_v01.jsonl / tests/session_facts.rs에서 잠김)
// ---------------------------------------------------------------------------

/// kind + subkind + payload를 지정해 이벤트를 시드한다 (system fact 카운트용).
async fn seed_event_with_payload(
    pool: &sqlx::SqlitePool,
    run_id: &str,
    session_id: &str,
    event_id: &str,
    kind: EventKind,
    subkind: Option<&str>,
    payload: serde_json::Value,
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
        actor: Actor::System,
        kind,
        subkind: subkind.map(String::from),
        parser_version: "test@v0".into(),
        payload,
        ..Default::default()
    };
    repo_observed::insert(pool, &e).await.unwrap();
}

#[tokio::test]
async fn system_summary_facts_counted_by_subkind() {
    // payload 형태는 session_facts_v01.jsonl 실 레코드와 동일 구조.
    let pool = test_pool().await;
    let run_id = repo_runs::start(&pool).await.unwrap();
    let sid = "sess_sysfacts";

    seed_event_with_payload(
        &pool,
        &run_id,
        sid,
        "td1",
        EventKind::SystemSummary,
        Some("turn_duration"),
        serde_json::json!({"durationMs": 139516, "messageCount": 100}),
    )
    .await;
    seed_event_with_payload(
        &pool,
        &run_id,
        sid,
        "td2",
        EventKind::SystemSummary,
        Some("turn_duration"),
        serde_json::json!({"durationMs": 454567, "messageCount": 334}),
    )
    .await;
    seed_event_with_payload(
        &pool,
        &run_id,
        sid,
        "ae1",
        EventKind::SystemSummary,
        Some("api_error"),
        serde_json::json!({"level": "error", "error": {"status": 529}}),
    )
    .await;
    seed_event_with_payload(
        &pool, &run_id, sid, "cb1",
        EventKind::SystemSummary, Some("compact_boundary"),
        serde_json::json!({"content": "Conversation compacted", "compactMetadata": {"trigger": "manual", "preTokens": 310705}}),
    ).await;
    // away_summary: 사용자 자리비움 turn — turn_duration 대신 기록된다.
    seed_event_with_payload(
        &pool,
        &run_id,
        sid,
        "aw1",
        EventKind::SystemSummary,
        Some("away_summary"),
        serde_json::json!({"content": "User stepped away"}),
    )
    .await;
    seed_event_with_payload(
        &pool,
        &run_id,
        sid,
        "aw2",
        EventKind::SystemSummary,
        Some("away_summary"),
        serde_json::json!({"content": "User stepped away"}),
    )
    .await;
    // 다른 subkind의 system 레코드는 어느 카운트에도 들어가지 않는다.
    seed_event_with_payload(
        &pool,
        &run_id,
        sid,
        "sh1",
        EventKind::SystemSummary,
        Some("stop_hook_summary"),
        serde_json::json!({}),
    )
    .await;

    let m = compute_session_metrics(&pool, sid).await.unwrap();
    assert_eq!(m.turn_duration_count, 2);
    assert_eq!(m.turn_duration_ms_total, 139516 + 454567);
    assert_eq!(m.api_error_count, 1);
    assert_eq!(m.compact_boundary_count, 1);
    // away_summary/compact_boundary는 활성 turn(turn_duration)이 아닌 별도 레코드다 —
    // away_summary_count로 노출해 turn_duration_count가 전체 turn이 아님을 정직화한다
    // (dogfooding 2026-06-11: 6a254a2a는 turn_duration 27 + away 10 + compact 1 = user_turns 38).
    assert_eq!(m.away_summary_count, 2);
}

#[tokio::test]
async fn truncated_tool_result_marker_counted() {
    let pool = test_pool().await;
    let run_id = repo_runs::start(&pool).await.unwrap();
    let sid = "sess_trunc";

    // 진성 마커 — 본문 중간 "\n\n... [N characters truncated] ...\n\n" (실 6건 전수 동일 형태).
    seed_tool_result_with_content(
        &pool,
        &run_id,
        sid,
        "tr1",
        "modified: a.md\n\n... [5882 characters truncated] ...\n\nmodified: b.md",
    )
    .await;
    // 숫자 대신 리터럴 N으로 인용된 문구(코퍼스의 문서 인용 형태)는 매칭되지 않는다.
    seed_tool_result_with_content(
        &pool,
        &run_id,
        sid,
        "tr2",
        "<code>... [N characters truncated] ...</code> 잘림 fact(6건)",
    )
    .await;
    // 마커 없는 일반 출력.
    seed_tool_result_with_content(&pool, &run_id, sid, "tr3", "test result: ok").await;

    let m = compute_session_metrics(&pool, sid).await.unwrap();
    assert_eq!(m.tool_result_truncated_count, 1);
}

/// user_message text 이벤트를 시드한다 (중단 마커 카운트용).
async fn seed_user_text(
    pool: &sqlx::SqlitePool,
    run_id: &str,
    session_id: &str,
    event_id: &str,
    text: &str,
) {
    seed_event_with_payload(
        pool,
        run_id,
        session_id,
        event_id,
        EventKind::UserMessage,
        None,
        serde_json::json!({"content_ordinal": 0, "text": text}),
    )
    .await;
}

#[tokio::test]
async fn user_interruption_markers_counted() {
    let pool = test_pool().await;
    let run_id = repo_runs::start(&pool).await.unwrap();
    let sid = "sess_interrupt";

    // 두 실측 변형 (session_facts_v01.jsonl) — 둘 다 카운트.
    seed_user_text(&pool, &run_id, sid, "ui1", "[Request interrupted by user]").await;
    seed_user_text(
        &pool,
        &run_id,
        sid,
        "ui2",
        "[Request interrupted by user for tool use]",
    )
    .await;
    // 일반 사용자 메시지 + mid-content 인용은 카운트되지 않는다.
    seed_user_text(&pool, &run_id, sid, "ui3", "please fix the bug").await;
    seed_user_text(
        &pool,
        &run_id,
        sid,
        "ui4",
        "코퍼스에서 [Request interrupted by user] 마커를 세어줘",
    )
    .await;

    let m = compute_session_metrics(&pool, sid).await.unwrap();
    assert_eq!(m.user_interruption_count, 2);
}

/// 대시보드 피드백(2026-07-04) — rate limit 횟수 + 토큰 사용량 노출.
///
/// api_rate_limit_count: api_error payload의 `/error/status`가 429인 것만
/// 센다(Anthropic API docs: 429 = rate_limit_error). payload 경로는 실측
/// 표본(이 DB 529 overloaded 11건 전수: `error.status` 숫자 필드)과 동일
/// 구조 — 상태값 분류만 docs 인용.
/// input/output/cache 토큰 합계: usage facet 세션 합계(session_aggregate와
/// 같은 측정면)를 SessionMetrics로 노출 — F1 원칙(count·합만)에 부합.
#[tokio::test]
async fn rate_limit_and_token_totals() {
    let pool = test_pool().await;
    let run_id = repo_runs::start(&pool).await.unwrap();
    let sid = "sess-rl";
    // 429 rate limit — 실측 529 구조(error.status)에서 status만 429.
    seed_event_with_payload(
        &pool,
        &run_id,
        sid,
        "e429",
        EventKind::SystemSummary,
        Some("api_error"),
        serde_json::json!({"error": {"status": 429, "message": "429 rate_limit_error"}}),
    )
    .await;
    // 529 overloaded — rate limit 아님(api_error_count에는 포함).
    seed_event_with_payload(
        &pool,
        &run_id,
        sid,
        "e529",
        EventKind::SystemSummary,
        Some("api_error"),
        serde_json::json!({"error": {"status": 529, "message": "529 Overloaded"}}),
    )
    .await;

    // usage facet 합계 배선 — 첫 compute 전에 삽입한다(캐시 키는
    // observed_event 기준이라 facet 단독 삽입은 키를 바꾸지 않음; 운영에선
    // facet이 이벤트와 같은 ingest flush로 들어와 안전).
    repo_usage_facet::insert(
        &pool,
        &repo_usage_facet::UsageFacetRow {
            raw_event_id: "raw_e429".into(),
            schema_version: "0.5.0".into(),
            session_id: sid.into(),
            model: Some("claude-opus-4-8".into()),
            input_tokens: 120,
            cache_creation_input_tokens: 30,
            cache_read_input_tokens: 4000,
            output_tokens: 250,
            observed_at: "2026-07-04T00:00:00Z".into(),
            parser_version: "usage@0.1".into(),
        },
    )
    .await
    .unwrap();

    let m = compute_session_metrics(&pool, sid).await.unwrap();
    assert_eq!(m.api_error_count, 2);
    assert_eq!(m.api_rate_limit_count, 1, "429만 rate limit으로 센다");
    assert_eq!(m.input_tokens, 120);
    assert_eq!(m.output_tokens, 250);
    assert_eq!(m.cache_read_input_tokens, 4000);
    assert_eq!(m.cache_creation_input_tokens, 30);
}
