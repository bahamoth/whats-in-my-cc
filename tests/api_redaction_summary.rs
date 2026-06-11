//! Slice-18 — Pull API envelope must include redaction_policy + redaction_summary.

use axum_test::TestServer;
use serde_json::Value;
use sqlx::sqlite::SqlitePoolOptions;
use wimcc::api::AppState;
use wimcc::db::migrate;
use wimcc::ingest::store;
use wimcc::live::NoopSink;

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
    TestServer::new(wimcc::api::router(state)).unwrap()
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

/// doc-audit-2026-06-10 — `/v1/sessions` meta.redaction_summary must
/// aggregate across *all* sessions in the response, not just the most
/// recent one. Setup: the redacted fixture session (timestamps 2026-06-01)
/// plus a newer clean session, so the clean session sorts first
/// (list_sessions orders by last_observed_at DESC) and a first-session-only
/// aggregate would report zero redactions.
#[tokio::test]
async fn sessions_redaction_summary_aggregates_across_all_sessions() {
    let pool = pool_with_redacted_session().await;

    let dir = tempfile::tempdir().unwrap();
    let clean = dir.path().join("clean_newer.jsonl");
    std::fs::write(
        &clean,
        concat!(
            r#"{"type":"user","uuid":"cn1","parentUuid":null,"sessionId":"sess-clean-newer","timestamp":"2026-06-08T00:00:00Z","cwd":"/tmp","gitBranch":"main","entrypoint":"cli","userType":"external","version":"2.1.144","isSidechain":false,"promptId":"p1","message":{"role":"user","content":"hello"}}"#,
            "\n",
            r#"{"type":"assistant","uuid":"cn2","parentUuid":"cn1","sessionId":"sess-clean-newer","timestamp":"2026-06-08T00:00:01Z","cwd":"/tmp","gitBranch":"main","entrypoint":"cli","userType":"external","version":"2.1.144","isSidechain":false,"requestId":"req_cn1","message":{"id":"msg_cn1","model":"claude-opus-4-7","type":"message","role":"assistant","stop_reason":"end_turn","content":[{"type":"text","text":"hi"}]}}"#,
            "\n",
        ),
    )
    .unwrap();
    store::ingest_file(&pool, &clean, &NoopSink).await.unwrap();

    let server = build_server(pool);
    let r = server.get("/v1/sessions").await;
    r.assert_status_ok();
    let body: Value = r.json();

    // Guard the test premise: the clean session must be listed first, so a
    // first-session-only aggregate cannot pass this test by accident.
    assert_eq!(
        body["data"][0]["session_id"], "sess-clean-newer",
        "clean session must sort first (last_observed_at DESC); got: {}",
        body["data"][0]["session_id"]
    );

    let total = body["meta"]["redaction_summary"]["total_items_redacted"]
        .as_u64()
        .expect("total_items_redacted must be a number");
    assert!(
        total >= 1,
        "summary must cover every listed session — the redacted fixture \
         session has >=1 redactions but is not first; got total={total}"
    );
}

#[tokio::test]
async fn session_events_endpoint_includes_redaction_summary() {
    let pool = pool_with_redacted_session().await;
    let server = build_server(pool.clone());

    // Get the session id
    let sid: String = sqlx::query_scalar("SELECT DISTINCT session_id FROM observed_event LIMIT 1")
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
    let sid: String = sqlx::query_scalar("SELECT DISTINCT session_id FROM observed_event LIMIT 1")
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
