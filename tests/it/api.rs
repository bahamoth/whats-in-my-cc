use axum_test::TestServer;
use serde_json::Value;
use sqlx::sqlite::SqlitePoolOptions;
use wimcc::db::migrate;
use wimcc::ingest::store;

async fn make_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    store::ingest_file(
        &pool,
        std::path::Path::new("tests/fixtures/transcripts/minimal_session.jsonl"),
        &wimcc::live::NoopSink,
    )
    .await
    .unwrap();
    pool
}

async fn setup() -> TestServer {
    let pool = make_pool().await;
    let app = wimcc::api::router(wimcc::api::AppState::new_for_tests(pool));
    TestServer::new(app).unwrap()
}

async fn setup_with_pool() -> (sqlx::SqlitePool, TestServer) {
    let pool = make_pool().await;
    let app = wimcc::api::router(wimcc::api::AppState::new_for_tests(pool.clone()));
    let server = TestServer::new(app).unwrap();
    (pool, server)
}

#[tokio::test]
async fn health() {
    let s = setup().await;
    let resp = s.get("/v1/health").await;
    resp.assert_status_ok();
    let v: Value = resp.json();
    assert_eq!(v["status"], "ok");
}

#[tokio::test]
async fn sessions_list_contains_sess_a() {
    let s = setup().await;
    let v: Value = s.get("/v1/sessions").await.json();
    assert_eq!(v["meta"]["schema_version"], "0.5.0");
    assert!(v["data"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["session_id"] == "sess-A"));
}

#[tokio::test]
async fn session_detail_reports_event_count() {
    let s = setup().await;
    let detail: Value = s.get("/v1/sessions/sess-A").await.json();
    assert!(detail["data"]["summary"]["event_count"].as_i64().unwrap() >= 6);
}

/// session_detail 404s when no observed_event rows exist for the id.
#[tokio::test]
async fn missing_session_detail_is_404() {
    let s = setup().await;
    s.get("/v1/sessions/missing")
        .await
        .assert_status(axum::http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn raw_endpoint_returns_record() {
    let (pool, server) = setup_with_pool().await;
    let event_id: String = sqlx::query_scalar("SELECT event_id FROM observed_event LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();

    let resp = server.get(&format!("/v1/events/{event_id}/raw")).await;
    resp.assert_status_ok();
    let body: serde_json::Value = resp.json();
    assert_eq!(body["data"]["event_id"], event_id);
    assert!(body["data"]["source"]["file_path"].is_string());
    assert_eq!(body["data"]["source"]["kind"], "claude_transcript");
    assert!(body["data"]["record"].is_object());
    assert!(body["data"]["record_type"].is_string());
    // doc-audit-2026-06-10 — must mirror raw_event.redaction_state (doc 05
    // vocabulary). The minimal fixture has no secrets and non-empty payloads,
    // so the ingest scan marks every row not_redacted.
    assert_eq!(body["data"]["redaction_state"], "not_redacted");
}

#[tokio::test]
async fn raw_endpoint_404_for_unknown_event() {
    let (pool, server) = setup_with_pool().await;
    drop(pool);
    let resp = server.get("/v1/events/no_such_event/raw").await;
    resp.assert_status_not_found();
}

#[tokio::test]
async fn telemetry_index_exists() {
    let pool = make_pool().await;
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM sqlite_master \
         WHERE type='index' AND name='idx_obs_trace_span'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.0, 1);
}

/// B-6c (2026-07-04) — `agent-setting` 레코드 정규화. real fixture
/// (teammate_v01/teammate_session_head.jsonl)의 첫 라인
/// `{"type":"agent-setting","agentSetting":"Explore",…}`(세션 e8b4a11e)가
/// session_state/agent_setting observed event로 승격되고, 세션 detail이
/// `agent_setting` 필드로 노출한다(배지 소비용 — 세션 상수라 live 집계).
#[tokio::test]
async fn agent_setting_record_is_normalised_and_surfaced_in_detail() {
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    store::ingest_file(
        &pool,
        std::path::Path::new(
            "tests/fixtures/transcripts/real/teammate_v01/teammate_session_head.jsonl",
        ),
        &wimcc::live::NoopSink,
    )
    .await
    .unwrap();
    let app = wimcc::api::router(wimcc::api::AppState::new_for_tests(pool));
    let s = TestServer::new(app).unwrap();
    let detail: Value = s
        .get("/v1/sessions/e8b4a11e-541d-4d64-9aae-52663c01c5cc")
        .await
        .json();
    assert_eq!(
        detail["data"]["agent_setting"], "Explore",
        "agent-setting must be normalised and surfaced; got: {detail}"
    );
}
