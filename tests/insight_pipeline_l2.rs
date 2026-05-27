//! Slice-15 — pipeline L2 integration: noop_test extractor queues to pending;
//! fixture judge drains the queue on subsequent rebuild.
//!
//! Uses the cfg(test) NoopTestExtractor registered inside run_extractors_with_runtime.

use sqlx::sqlite::SqlitePoolOptions;
use witmcc::db::migrate;
use witmcc::insight::judge::runtime::JudgeRuntime;
use witmcc::insight::pipeline::run_extractors_with_runtime;

/// A minimal migrated pool. No events needed for NoopTestExtractor (it emits
/// a synthetic candidate regardless of what events are in the DB).
async fn empty_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&pool)
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    pool
}

/// Pool with a tool_failure-triggering event. Used only by
/// l1_categories_unaffected_by_noop_judge which needs actual observed_event rows.
async fn tool_failure_pool() -> sqlx::SqlitePool {
    let pool = empty_pool().await;
    let now = "2026-01-01T00:00:00Z";

    // ingest_run (FK parent of raw_event)
    sqlx::query(
        "INSERT OR IGNORE INTO ingest_run (run_id, started_at, status) VALUES (?,?,?)",
    )
    .bind("run_0")
    .bind(now)
    .bind("ok")
    .execute(&pool)
    .await
    .unwrap();

    // raw_event — all NOT NULL columns supplied
    sqlx::query(
        "INSERT OR IGNORE INTO raw_event \
         (raw_event_id, ingest_run_id, source_type, source_uri, source_line_no, \
          source_byte_offset, payload_sha256, payload, captured_at) \
         VALUES (?,?,?,?,?,?,?,?,?)",
    )
    .bind("raw_000")
    .bind("run_0")
    .bind("claude_transcript")
    .bind("file://test.jsonl")
    .bind(0_i64)
    .bind(0_i64)
    .bind("aaa")
    .bind(b"{}" as &[u8])
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();

    // observed_event — tool_result with is_error=true inside the payload JSON
    // (tool_failure extractor reads payload->tool_result->is_error)
    let payload = serde_json::to_string(&serde_json::json!({
        "tool_result": {
            "tool_use_id": "tu_0",
            "is_error": true,
            "content": "command failed"
        }
    }))
    .unwrap();
    sqlx::query(
        "INSERT OR IGNORE INTO observed_event \
         (event_id, raw_event_id, schema_version, session_id, observed_at, \
          actor, kind, tool_use_id, is_sidechain, is_meta, payload, parser_version) \
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind("ev_000")
    .bind("raw_000")
    .bind("observed_event.v1")
    .bind("sess_t")
    .bind(now)
    .bind("tool")
    .bind("tool_result")
    .bind("tu_0")
    .bind(0_i64)
    .bind(0_i64)
    .bind(payload)
    .bind("v1")
    .execute(&pool)
    .await
    .unwrap();

    pool
}

#[tokio::test]
async fn noop_judge_queues_noop_test_candidate_to_pending() {
    let pool = empty_pool().await;
    let runtime = JudgeRuntime::noop();
    run_extractors_with_runtime(&pool, "sess_t", &runtime)
        .await
        .unwrap();

    let pending: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM findings_pending_judge")
        .fetch_one(&pool)
        .await
        .unwrap();
    // noop_test extractor (cfg(test)) emits 1 candidate; it goes to pending with NoopJudge
    assert!(pending >= 1, "expected >=1 pending, got {pending}");
}

#[tokio::test]
async fn fixture_judge_drains_pending_from_prior_run() {
    let pool = empty_pool().await;

    // First pass: noop judge — fills pending
    let noop = JudgeRuntime::noop();
    run_extractors_with_runtime(&pool, "sess_t", &noop)
        .await
        .unwrap();

    let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM findings_pending_judge")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(before >= 1, "pending should have entries after noop pass");

    // Second pass: fixture judge — drains pending
    let fixture = JudgeRuntime::fixture(
        std::path::Path::new("tests/fixtures/judge/scenario_a.json"),
        20,
    )
    .unwrap();
    run_extractors_with_runtime(&pool, "sess_t", &fixture)
        .await
        .unwrap();

    let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM findings_pending_judge")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        after, 0,
        "pending should be empty after fixture judge drains it"
    );
}

#[tokio::test]
async fn budget_exhaustion_leaves_items_in_pending() {
    let pool = empty_pool().await;
    let fixture = JudgeRuntime::fixture_with_budget(
        std::path::Path::new("tests/fixtures/judge/scenario_a.json"),
        0, // zero budget — everything queues
    )
    .unwrap();
    run_extractors_with_runtime(&pool, "sess_t", &fixture)
        .await
        .unwrap();

    let pending: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM findings_pending_judge")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(pending >= 1, "budget=0 should leave candidates in pending");
}

#[tokio::test]
async fn l1_categories_unaffected_by_noop_judge() {
    // tool_failure (L1/Always policy) must still write findings even when judge is noop.
    let pool = tool_failure_pool().await;

    let runtime = JudgeRuntime::noop();
    run_extractors_with_runtime(&pool, "sess_t", &runtime)
        .await
        .unwrap();

    let findings: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM finding WHERE category='tool_failure'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(findings >= 1, "tool_failure L1 must still fire with noop judge");
}
