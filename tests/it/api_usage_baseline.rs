//! GET /v1/usage/baseline returns the cross-session median (+ p25/p75) of the
//! key usage metrics. Seeds two sessions with known values and asserts the
//! median is computed correctly in Rust (SQLite has no MEDIAN()).
use axum_test::TestServer;
use serde_json::Value;
use sqlx::sqlite::SqlitePoolOptions;
use wimcc::api::{router, AppState};
use wimcc::db::repo_signal::SignalRow;
use wimcc::db::repo_usage_facet::UsageFacetRow;
use wimcc::db::repo_verification_run::VerificationRunRow;
use wimcc::db::{migrate, repo_raw, repo_signal, repo_usage_facet, repo_verification_run};
use wimcc::model::observed::{Actor, EventKind, ObservedEvent};

async fn empty_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    pool
}

fn uf(
    raw_event_id: &str,
    session_id: &str,
    input: i64,
    cc: i64,
    cr: i64,
    output: i64,
) -> UsageFacetRow {
    UsageFacetRow {
        raw_event_id: raw_event_id.into(),
        schema_version: "usage_facet.v1".into(),
        session_id: session_id.into(),
        model: Some("claude-opus-4-8".into()),
        input_tokens: input,
        cache_creation_input_tokens: cc,
        cache_read_input_tokens: cr,
        output_tokens: output,
        observed_at: "2026-05-30T10:00:00Z".into(),
        parser_version: "usage_facet@v1".into(),
    }
}

#[tokio::test]
async fn baseline_endpoint_returns_median_across_sessions() {
    let pool = empty_pool().await;

    // Session 1: assistant_events=1, billed = 100 + 0 + 100 = 200, output 100,
    //   denom = cr(0)+cc(0)+input(100)=100, cache_hit_ratio = 0/100 = 0.0
    repo_usage_facet::insert(&pool, &uf("r1", "sess_lo", 100, 0, 0, 100))
        .await
        .unwrap();
    // Session 2: assistant_events=1, billed = 100 + 0 + 300 = 400, output 300,
    //   denom = cr(900)+cc(0)+input(100)=1000, cache_hit_ratio = 900/1000 = 0.9
    repo_usage_facet::insert(&pool, &uf("r2", "sess_hi", 100, 0, 900, 300))
        .await
        .unwrap();

    let state = AppState::new_for_tests(pool);
    let server = TestServer::new(router(state)).unwrap();
    let r = server.get("/v1/usage/baseline").await;
    r.assert_status_ok();
    let body = r.json::<Value>();
    let data = &body["data"];

    assert_eq!(data["session_count"].as_i64().unwrap(), 2);
    // Two values -> median is the midpoint (type-7 interpolation).
    // billed_tokens: [200, 400] -> median 300.
    assert_eq!(data["billed_tokens"]["median"].as_f64().unwrap(), 300.0);
    // output_tokens: [100, 300] -> median 200.
    assert_eq!(data["output_tokens"]["median"].as_f64().unwrap(), 200.0);
    // assistant_events: [1, 1] -> median 1.
    assert_eq!(data["assistant_events"]["median"].as_f64().unwrap(), 1.0);
    // cache_hit_ratio: [0.0, 0.9] -> median 0.45.
    let chr = data["cache_hit_ratio"]["median"].as_f64().unwrap();
    assert!((chr - 0.45).abs() < 1e-9, "got {chr}");
    // p25/p75 present (not null) when there is data.
    assert!(data["billed_tokens"]["p25"].as_f64().is_some());
    assert!(data["billed_tokens"]["p75"].as_f64().is_some());
}

#[tokio::test]
async fn baseline_endpoint_empty_store_returns_nulls() {
    let pool = empty_pool().await;
    let state = AppState::new_for_tests(pool);
    let server = TestServer::new(router(state)).unwrap();
    let r = server.get("/v1/usage/baseline").await;
    r.assert_status_ok();
    let body = r.json::<Value>();
    let data = &body["data"];
    assert_eq!(data["session_count"].as_i64().unwrap(), 0);
    assert!(data["billed_tokens"]["median"].is_null());
    assert!(data["cache_hit_ratio"]["median"].is_null());
}

// ---------------------------------------------------------------------------
// PR-3 §3a — session_id project scope + 5-metric sample n
// ---------------------------------------------------------------------------

/// observed_event 1건 seed (raw FK 포함) — cwd를 채워 project 파생을 만든다.
async fn seed_event_with_cwd(
    pool: &sqlx::SqlitePool,
    session_id: &str,
    event_id: &str,
    kind: EventKind,
    cwd: &str,
) {
    // ingest_run FK for raw_event — idempotent so the helper can be called per event.
    sqlx::query(
        "INSERT OR IGNORE INTO ingest_run(run_id, started_at, status) VALUES(?, ?, 'running')",
    )
    .bind("run_baseline_test")
    .bind(chrono::Utc::now().to_rfc3339())
    .execute(pool)
    .await
    .unwrap();
    let raw_id = format!("raw_{event_id}");
    repo_raw::insert_dedup(
        pool,
        &repo_raw::NewRaw {
            raw_event_id: raw_id.clone(),
            ingest_run_id: "run_baseline_test".into(),
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
        cwd: Some(cwd.into()),
        parser_version: "test@v0".into(),
        ..Default::default()
    };
    wimcc::db::repo_observed::insert(pool, &e).await.unwrap();
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

#[tokio::test]
async fn baseline_stat_carries_sample_n() {
    let pool = empty_pool().await;
    repo_usage_facet::insert(&pool, &uf("r1", "s1", 100, 0, 0, 100))
        .await
        .unwrap();
    repo_usage_facet::insert(&pool, &uf("r2", "s2", 100, 0, 900, 300))
        .await
        .unwrap();

    let state = AppState::new_for_tests(pool);
    let server = TestServer::new(router(state)).unwrap();
    let body = server.get("/v1/usage/baseline").await.json::<Value>();
    let data = &body["data"];
    // usage 4지표: n = usage 세션 수. cache_hit_ratio도 두 세션 모두 분모>0이라 n=2.
    assert_eq!(data["billed_tokens"]["n"].as_i64().unwrap(), 2);
    assert_eq!(data["cache_hit_ratio"]["n"].as_i64().unwrap(), 2);
    // 파라미터 없음 → store 스코프.
    assert_eq!(data["scope"].as_str().unwrap(), "store");
    assert!(data["project"].is_null());
}

#[tokio::test]
async fn baseline_new_stats_gate_their_denominators() {
    let pool = empty_pool().await;
    // s1: 검증 passed 1 + failed 1 (측정 2), tool_call 2회 + tool_failure 시그널 1.
    repo_usage_facet::insert(&pool, &uf("r1", "s1", 100, 0, 0, 100))
        .await
        .unwrap();
    seed_event_with_cwd(&pool, "s1", "e1", EventKind::ToolCall, "/proj/a").await;
    seed_event_with_cwd(&pool, "s1", "e2", EventKind::ToolCall, "/proj/a").await;
    repo_signal::insert(&pool, &make_signal("s1", "sig1", "tool_failure"))
        .await
        .unwrap();
    repo_verification_run::insert(&pool, &make_vrun("s1", "v1", "passed"))
        .await
        .unwrap();
    repo_verification_run::insert(&pool, &make_vrun("s1", "v2", "failed"))
        .await
        .unwrap();
    // s2: usage만 있음 — 검증 0건·tool_call 0건 → pass_rate/tool_failure 분포에서 제외.
    repo_usage_facet::insert(&pool, &uf("r2", "s2", 100, 0, 900, 300))
        .await
        .unwrap();

    let state = AppState::new_for_tests(pool);
    let server = TestServer::new(router(state)).unwrap();
    let body = server.get("/v1/usage/baseline").await.json::<Value>();
    let data = &body["data"];
    // pass_rate: s1만 측정(1/2=0.5), n=1.
    assert_eq!(data["verification_pass_rate"]["n"].as_i64().unwrap(), 1);
    assert!((data["verification_pass_rate"]["median"].as_f64().unwrap() - 0.5).abs() < 1e-9);
    // tool_failure_count: tool_call>0인 s1만, 값 1, n=1.
    assert_eq!(data["tool_failure_count"]["n"].as_i64().unwrap(), 1);
    assert_eq!(data["tool_failure_count"]["median"].as_f64().unwrap(), 1.0);
    // estimated_cost_usd: billed>0인 s1·s2, n=2, median>0 (opus-4-8 가격표 有).
    assert_eq!(data["estimated_cost_usd"]["n"].as_i64().unwrap(), 2);
    assert!(data["estimated_cost_usd"]["median"].as_f64().unwrap() > 0.0);
}

#[tokio::test]
async fn baseline_scopes_to_the_sessions_project() {
    let pool = empty_pool().await;
    // 프로젝트 A 세션 2개, 프로젝트 B 세션 1개 — usage 값이 뚜렷이 다름.
    repo_usage_facet::insert(&pool, &uf("r1", "sa1", 100, 0, 0, 100))
        .await
        .unwrap(); // billed 200
    repo_usage_facet::insert(&pool, &uf("r2", "sa2", 200, 0, 0, 200))
        .await
        .unwrap(); // billed 400
    repo_usage_facet::insert(&pool, &uf("r3", "sb1", 9000, 0, 0, 9000))
        .await
        .unwrap(); // billed 18000
    seed_event_with_cwd(&pool, "sa1", "ea1", EventKind::AssistantMessage, "/proj/a").await;
    seed_event_with_cwd(&pool, "sa2", "ea2", EventKind::AssistantMessage, "/proj/a").await;
    seed_event_with_cwd(&pool, "sb1", "eb1", EventKind::AssistantMessage, "/proj/b").await;
    for sid in ["sa1", "sa2", "sb1"] {
        wimcc::db::repo_observed::upsert_session_summary(&pool, sid)
            .await
            .unwrap();
    }

    let state = AppState::new_for_tests(pool);
    let server = TestServer::new(router(state)).unwrap();
    let body = server
        .get("/v1/usage/baseline")
        .add_query_param("session_id", "sa1")
        .await
        .json::<Value>();
    let data = &body["data"];
    assert_eq!(data["scope"].as_str().unwrap(), "project");
    assert_eq!(data["project"].as_str().unwrap(), "/proj/a");
    // 프로젝트 A만: billed [200,400] → median 300 (B의 18000이 섞이면 400).
    assert_eq!(data["billed_tokens"]["median"].as_f64().unwrap(), 300.0);
    assert_eq!(data["session_count"].as_i64().unwrap(), 2);
}

// PR-3 §3a — session_id가 왔지만 그 세션의 project를 해석할 수 없으면(관측
// 이벤트/cwd 없음 → session_summary.project 미상) store 전체로 정직하게 폴백한다.
#[tokio::test]
async fn baseline_session_id_unknown_project_falls_back_to_store() {
    let pool = empty_pool().await;
    // usage만 있고 관측 이벤트가 없는 세션들 → project 해석 불가.
    repo_usage_facet::insert(&pool, &uf("r1", "s1", 100, 0, 0, 100))
        .await
        .unwrap(); // billed 200
    repo_usage_facet::insert(&pool, &uf("r2", "s2", 300, 0, 0, 300))
        .await
        .unwrap(); // billed 600

    let state = AppState::new_for_tests(pool);
    let server = TestServer::new(router(state)).unwrap();
    let body = server
        .get("/v1/usage/baseline")
        .add_query_param("session_id", "s1")
        .await
        .json::<Value>();
    let data = &body["data"];
    // project 미상 → store 스코프로 폴백(필터 미적용, 정직하게 표기).
    assert_eq!(data["scope"].as_str().unwrap(), "store");
    assert!(data["project"].is_null());
    // 전체 billed [200,600] → median 400 (스코프 필터가 걸렸다면 s1만 남아 200).
    assert_eq!(data["billed_tokens"]["median"].as_f64().unwrap(), 400.0);
    assert_eq!(data["session_count"].as_i64().unwrap(), 2);
}
