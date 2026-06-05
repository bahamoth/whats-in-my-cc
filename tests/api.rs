use axum_test::TestServer;
use serde_json::Value;
use sqlx::sqlite::SqlitePoolOptions;
use wimcc::db::migrate;
use wimcc::graph::build;
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
    build::rebuild_session(&pool, "sess-A").await.unwrap();
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
async fn session_detail_and_graph() {
    let s = setup().await;
    let detail: Value = s.get("/v1/sessions/sess-A").await.json();
    assert!(detail["data"]["summary"]["event_count"].as_i64().unwrap() >= 6);
    let graph: Value = s.get("/v1/sessions/sess-A/graph").await.json();
    let nodes = graph["data"]["nodes"].as_array().unwrap();
    let edges = graph["data"]["edges"].as_array().unwrap();
    assert!(!nodes.is_empty());
    assert!(!edges.is_empty());
}

/// Slice-8 — session_graph now always returns 200 (empty graph is a valid
/// transient state during ingest's rebuild_session race). Use session_detail
/// for the "session not found" path instead — that handler still 404s when
/// no observed_event rows exist for the id.
#[tokio::test]
async fn missing_session_detail_is_404() {
    let s = setup().await;
    s.get("/v1/sessions/missing")
        .await
        .assert_status(axum::http::StatusCode::NOT_FOUND);
}

/// session_graph returns 200 + empty graph for unknown sessions (slice-8).
#[tokio::test]
async fn missing_session_graph_is_200_empty() {
    let s = setup().await;
    let resp = s.get("/v1/sessions/missing/graph").await;
    resp.assert_status_ok();
    let v: Value = resp.json();
    assert!(v["data"]["nodes"].as_array().unwrap().is_empty());
    assert!(v["data"]["edges"].as_array().unwrap().is_empty());
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
    assert_eq!(body["data"]["redaction_state"], "none");
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

#[tokio::test]
async fn hook_post_accepts_single_pretooluse() {
    let s = setup().await;
    let body = serde_json::json!({
        "session_id":      "sess_HK1",
        "hook_event_name": "PreToolUse",
        "tool_name":       "Bash",
        "tool_input":      {"command": "ls"},
        "tool_use_id":     "toolu_HK1"
    });
    let resp = s.post("/hooks/v1/events").json(&body).await;
    resp.assert_status_ok();
    let v: Value = resp.json();
    assert_eq!(v["data"]["accepted_events"], 1);
    assert_eq!(v["data"]["rejected_events"], 0);
    assert_eq!(v["data"]["duplicate_events"], 0);
    assert_eq!(v["data"]["sessions_touched"][0], "sess_HK1");

    // Slice-9 — session_detail no longer ships events. Verify the hook landed
    // by fetching the new windowed events endpoint instead.
    let events = s.get("/v1/sessions/sess_HK1/events").await;
    events.assert_status_ok();
    let ev: Value = events.json();
    let has_hook = ev["data"]["events"].as_array().unwrap().iter().any(|e| {
        e["kind"] == "hook_event" && e["subkind"] == "pre_tool_use"
    });
    assert!(has_hook, "hook_event with subkind=pre_tool_use missing");
}

#[tokio::test]
async fn hook_post_rejects_missing_session_id() {
    let s = setup().await;
    let body = serde_json::json!({
        "hook_event_name": "PreToolUse"
    });
    let resp = s.post("/hooks/v1/events").json(&body).await;
    resp.assert_status_ok();
    let v: Value = resp.json();
    assert_eq!(v["data"]["accepted_events"], 0);
    assert_eq!(v["data"]["rejected_events"], 1);
}
