//! GET /v1/metrics — 프로젝트/기간 필터의 세션 횡단 metrics+fingerprint series (Task 5).
//!
//! 전후 비교(개입 효과 귀속)의 측정면. count only — rate·판단은 소비자 몫.
//! 미지원 쿼리 파라미터는 400 (deny_unknown_fields — dogfood 2026-06-12 계약).

use serde_json::Value;
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

async fn seed_tool_call(
    pool: &sqlx::SqlitePool,
    run_id: &str,
    sid: &str,
    eid: &str,
    cwd: &str,
    observed_at: chrono::DateTime<chrono::Utc>,
) {
    let raw_id = format!("raw_{eid}");
    repo_raw::insert_dedup(
        pool,
        &repo_raw::NewRaw {
            raw_event_id: raw_id.clone(),
            ingest_run_id: run_id.into(),
            source_type: "claude_transcript".into(),
            source_uri: "/tmp/ms.jsonl".into(),
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
        observed_at,
        actor: Actor::Assistant,
        kind: EventKind::ToolCall,
        cwd: Some(cwd.into()),
        parser_version: "test@v0".into(),
        ..Default::default()
    };
    repo_observed::insert(pool, &e).await.unwrap();
}

fn t(rfc3339: &str) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339(rfc3339)
        .unwrap()
        .with_timezone(&chrono::Utc)
}

async fn seed_three_sessions(pool: &sqlx::SqlitePool) {
    let run = repo_runs::start(pool).await.unwrap();
    // 프로젝트 A: 6/1과 6/10 두 세션, 프로젝트 B: 6/5 한 세션.
    seed_tool_call(
        pool,
        &run,
        "sess_a_old",
        "ms1",
        "/proj/A",
        t("2026-06-01T00:00:00Z"),
    )
    .await;
    seed_tool_call(
        pool,
        &run,
        "sess_a_new",
        "ms2",
        "/proj/A",
        t("2026-06-10T00:00:00Z"),
    )
    .await;
    seed_tool_call(
        pool,
        &run,
        "sess_a_new",
        "ms3",
        "/proj/A",
        t("2026-06-10T01:00:00Z"),
    )
    .await;
    seed_tool_call(
        pool,
        &run,
        "sess_b",
        "ms4",
        "/proj/B",
        t("2026-06-05T00:00:00Z"),
    )
    .await;
}

fn server(pool: sqlx::SqlitePool) -> axum_test::TestServer {
    axum_test::TestServer::new(wimcc::api::router(AppState::new_for_tests(pool))).unwrap()
}

#[tokio::test]
async fn metrics_series_filters_by_project_and_includes_metrics_and_fingerprint() {
    let pool = test_pool().await;
    seed_three_sessions(&pool).await;
    let s = server(pool);
    let r = s
        .get("/v1/metrics")
        .add_query_param("project", "/proj/A")
        .await;
    r.assert_status_ok();
    let body: Value = r.json();
    assert!(body["meta"]["schema_version"].is_string());
    let sessions = body["data"]["sessions"].as_array().unwrap();
    assert_eq!(sessions.len(), 2);
    // 최신(last_observed) 우선 정렬.
    assert_eq!(sessions[0]["session_id"], "sess_a_new");
    assert_eq!(
        sessions[0]["metrics"]["tool_call_total"].as_i64().unwrap(),
        2
    );
    assert_eq!(sessions[0]["fingerprint"]["cwds"][0], "/proj/A");
    assert_eq!(body["data"]["session_count"].as_i64().unwrap(), 2);
    assert_eq!(body["data"]["matched_count"].as_i64().unwrap(), 2);
    // F1: rate 필드 금지.
    assert!(sessions[0]["metrics"].get("tool_failure_rate").is_none());
}

#[tokio::test]
async fn metrics_series_filters_by_time_window() {
    let pool = test_pool().await;
    seed_three_sessions(&pool).await;
    let s = server(pool);
    let r = s
        .get("/v1/metrics")
        .add_query_param("project", "/proj/A")
        .add_query_param("from", "2026-06-05T00:00:00Z")
        .await;
    r.assert_status_ok();
    let body: Value = r.json();
    let sessions = body["data"]["sessions"].as_array().unwrap();
    assert_eq!(sessions.len(), 1, "from 이후 first_observed 세션만");
    assert_eq!(sessions[0]["session_id"], "sess_a_new");

    let r2 = s
        .get("/v1/metrics")
        .add_query_param("to", "2026-06-04T00:00:00Z")
        .await;
    r2.assert_status_ok();
    let body2: Value = r2.json();
    let s2 = body2["data"]["sessions"].as_array().unwrap();
    assert_eq!(s2.len(), 1);
    assert_eq!(s2[0]["session_id"], "sess_a_old");
}

#[tokio::test]
async fn metrics_series_rejects_unknown_params_and_bad_time() {
    let pool = test_pool().await;
    let s = server(pool);
    let r = s.get("/v1/metrics").add_query_param("nope", "1").await;
    r.assert_status(axum::http::StatusCode::BAD_REQUEST);
    let r2 = s
        .get("/v1/metrics")
        .add_query_param("from", "yesterday")
        .await;
    r2.assert_status(axum::http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn metrics_series_limit_truncates_but_reports_matched() {
    let pool = test_pool().await;
    seed_three_sessions(&pool).await;
    let s = server(pool);
    let r = s.get("/v1/metrics").add_query_param("limit", "1").await;
    r.assert_status_ok();
    let body: Value = r.json();
    assert_eq!(body["data"]["sessions"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["session_count"].as_i64().unwrap(), 1);
    assert_eq!(body["data"]["matched_count"].as_i64().unwrap(), 3);
}
