//! Slice-15 — pipeline L2 integration: noop_test extractor queues to pending;
//! fixture judge drains the queue on subsequent rebuild.
//!
//! Uses the cfg(test) NoopTestExtractor registered inside run_extractors_with_runtime.

use sqlx::sqlite::SqlitePoolOptions;
use witmcc::db::migrate;
use witmcc::insight::judge::runtime::JudgeRuntime;
use witmcc::insight::pipeline::run_extractors_with_runtime;

async fn seeded_pool() -> sqlx::SqlitePool {
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

    // Insert a minimal session with one event so NoopTestExtractor emits a candidate.
    let sess = "sess_t";
    let now = "2026-01-01T00:00:00Z";
    sqlx::query(
        "INSERT OR IGNORE INTO raw_event \
         (raw_event_id,ingest_run_id,source_type,source_uri,source_line_no,\
          captured_at,payload_json,schema_version,provenance) \
         VALUES (?,?,?,?,?,?,?,?,?)",
    )
    .bind("raw_000")
    .bind("run_0")
    .bind("claude_transcript")
    .bind("file://test.jsonl")
    .bind(0_i64)
    .bind(now)
    .bind("{}")
    .bind("raw_event.v1")
    .bind("{}")
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT OR IGNORE INTO observed_event \
         (event_id,raw_event_id,schema_version,session_id,observed_at,\
          actor,kind,tool_name,tool_use_id,is_error,provenance) \
         VALUES (?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind("ev_000")
    .bind("raw_000")
    .bind("observed_event.v1")
    .bind(sess)
    .bind(now)
    .bind("tool")
    .bind("tool_use")
    .bind("Bash")
    .bind("tu_0")
    .bind(0_i64)
    .bind("{}")
    .execute(&pool)
    .await
    .unwrap();

    pool
}

#[tokio::test]
async fn noop_judge_queues_noop_test_candidate_to_pending() {
    let pool = seeded_pool().await;
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
    let pool = seeded_pool().await;

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
    let pool = seeded_pool().await;
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
    let pool = seeded_pool().await;
    // Insert a tool_result with is_error=1 and no retry — triggers tool_failure L1
    let now = "2026-01-01T00:00:01Z";
    sqlx::query(
        "INSERT OR IGNORE INTO raw_event \
         (raw_event_id,ingest_run_id,source_type,source_uri,source_line_no,\
          captured_at,payload_json,schema_version,provenance) \
         VALUES (?,?,?,?,?,?,?,?,?)",
    )
    .bind("raw_001")
    .bind("run_0")
    .bind("claude_transcript")
    .bind("file://test.jsonl")
    .bind(1_i64)
    .bind(now)
    .bind("{}")
    .bind("raw_event.v1")
    .bind("{}")
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT OR IGNORE INTO observed_event \
         (event_id,raw_event_id,schema_version,session_id,observed_at,\
          actor,kind,tool_name,tool_use_id,is_error,provenance) \
         VALUES (?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind("ev_001")
    .bind("raw_001")
    .bind("observed_event.v1")
    .bind("sess_t")
    .bind(now)
    .bind("tool")
    .bind("tool_result")
    .bind("Bash")
    .bind("tu_0")
    .bind(1_i64)
    .bind("{}")
    .execute(&pool)
    .await
    .unwrap();

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
