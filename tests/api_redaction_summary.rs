//! Slice-18 — Pull API envelope must include redaction_policy + redaction_summary.

use axum_test::TestServer;
use serde_json::Value;
use sqlx::sqlite::SqlitePoolOptions;
use witmcc::api::AppState;
use witmcc::db::migrate;
use witmcc::ingest::store;
use witmcc::live::NoopSink;

async fn pool_with_redacted_session() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    store::ingest_file(
        &pool,
        std::path::Path::new("tests/fixtures/redaction/synthetic_secrets.jsonl"),
        &NoopSink,
    )
    .await
    .unwrap();
    pool
}

fn build_server(pool: sqlx::SqlitePool) -> TestServer {
    let state = AppState::new_for_tests(pool);
    TestServer::new(witmcc::api::router(state)).unwrap()
}

#[tokio::test]
async fn sessions_endpoint_includes_redaction_policy() {
    let pool = pool_with_redacted_session().await;
    let server = build_server(pool);
    let r = server.get("/v1/sessions").await;
    r.assert_status_ok();
    let body: Value = r.json();
    assert!(
        body["meta"]["redaction_policy"].is_object(),
        "meta.redaction_policy must be an object; got: {}",
        body["meta"]
    );
    assert_eq!(
        body["meta"]["redaction_policy"]["applied"],
        Value::Bool(true),
        "redaction_policy.applied must be true"
    );
}

#[tokio::test]
async fn session_events_endpoint_includes_redaction_summary() {
    let pool = pool_with_redacted_session().await;
    let server = build_server(pool.clone());

    // Get the session id
    let sid: String =
        sqlx::query_scalar("SELECT DISTINCT session_id FROM observed_event LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();

    let r = server.get(&format!("/v1/sessions/{sid}/events")).await;
    r.assert_status_ok();
    let body: Value = r.json();
    assert!(
        body["meta"]["redaction_summary"].is_object(),
        "meta.redaction_summary must be an object; got: {}",
        body["meta"]
    );
    let total = body["meta"]["redaction_summary"]["total_items_redacted"]
        .as_u64()
        .expect("total_items_redacted must be a number");
    assert!(
        total >= 1,
        "total_items_redacted must be >= 1 for the secrets fixture; got: {total}"
    );
}

#[tokio::test]
async fn session_events_redaction_summary_has_required_fields() {
    let pool = pool_with_redacted_session().await;
    let server = build_server(pool.clone());
    let sid: String =
        sqlx::query_scalar("SELECT DISTINCT session_id FROM observed_event LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
    let r = server.get(&format!("/v1/sessions/{sid}/events")).await;
    r.assert_status_ok();
    let body: Value = r.json();
    let summary = &body["meta"]["redaction_summary"];
    assert!(
        summary["total_items_redacted"].is_number(),
        "total_items_redacted must be a number"
    );
    assert!(
        summary["rules_seen"].is_array(),
        "rules_seen must be an array"
    );
    assert!(
        summary["any_unredacted_sensitive"].is_boolean(),
        "any_unredacted_sensitive must be a boolean"
    );
}
