use axum_test::TestServer;
use serde_json::Value;
use sqlx::sqlite::SqlitePoolOptions;
use witmcc::db::migrate;
use witmcc::graph::build;
use witmcc::ingest::store;

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
    )
    .await
    .unwrap();
    build::rebuild_session(&pool, "sess-A").await.unwrap();
    pool
}

async fn setup() -> TestServer {
    let pool = make_pool().await;
    let app = witmcc::api::router(pool);
    TestServer::new(app).unwrap()
}

async fn setup_with_pool() -> (sqlx::SqlitePool, TestServer) {
    let pool = make_pool().await;
    let app = witmcc::api::router(pool.clone());
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
    assert_eq!(v["meta"]["schema_version"], "0.1.0");
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

#[tokio::test]
async fn missing_session_is_404() {
    let s = setup().await;
    s.get("/v1/sessions/missing/graph")
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
    assert_eq!(body["data"]["redaction_state"], "none");
}

#[tokio::test]
async fn raw_endpoint_404_for_unknown_event() {
    let (pool, server) = setup_with_pool().await;
    drop(pool);
    let resp = server.get("/v1/events/no_such_event/raw").await;
    resp.assert_status_not_found();
}
