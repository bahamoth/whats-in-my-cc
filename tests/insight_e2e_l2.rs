//! Slice-16 — end-to-end L2 pipeline tests for the three new categories.
//!
//! Each test:
//! 1. Seeds a synthetic DB with events that trigger (or don't) the category.
//! 2. Seeds the FixtureJudge with a pre-recorded verdict.
//! 3. Runs the pipeline with FixtureJudge.
//! 4. Asserts finding count matches golden expectation.
//!
//! No real LLM calls are made. The FixtureJudge reads from
//! `tests/fixtures/judge/scenario_combined.json`.

use sqlx::sqlite::SqlitePoolOptions;
use witmcc::db::migrate;
use witmcc::db::repo_diff_hunk::NewDiffHunk;
use witmcc::db::repo_verification_run::VerificationRunRow;
use witmcc::insight::judge::cache::evidence_hash;
use witmcc::insight::judge::runtime::JudgeRuntime;
use witmcc::insight::pipeline::run_extractors_with_runtime;

const FIXTURE_JUDGE_PATH: &str = "tests/fixtures/judge/scenario_combined.json";

async fn make_pool() -> sqlx::SqlitePool {
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

async fn insert_raw(pool: &sqlx::SqlitePool, raw_id: &str, ts: &str) {
    sqlx::query(
        "INSERT OR IGNORE INTO raw_event \
         (raw_event_id, ingest_run_id, source_type, source_uri, source_line_no, \
          source_byte_offset, payload_sha256, payload, captured_at) \
         VALUES (?,?,?,?,?,?,?,?,?)",
    )
    .bind(raw_id)
    .bind("run_0")
    .bind("claude_transcript")
    .bind("test.jsonl")
    .bind(0i64)
    .bind(0i64)
    .bind(format!("sha256_{raw_id}"))
    .bind(b"{}" as &[u8])
    .bind(ts)
    .execute(pool)
    .await
    .unwrap();
}

// ---------------------------------------------------------------------------
// risky_action positive: destructive Bash command
// ---------------------------------------------------------------------------

async fn seed_risky_action_positive(pool: &sqlx::SqlitePool, sess: &str) {
    let ts = "2026-01-01T00:00:00Z";
    insert_raw(pool, "raw_000", ts).await;
    insert_raw(pool, "raw_001", ts).await;

    // tool_call: rm -rf /tmp/foo
    let call_payload = serde_json::to_string(&serde_json::json!({
        "tool_use": {
            "name": "Bash",
            "input": { "command": "rm -rf /tmp/foo" }
        }
    }))
    .unwrap();
    sqlx::query(
        "INSERT OR IGNORE INTO observed_event \
         (event_id, raw_event_id, schema_version, session_id, observed_at, \
          actor, kind, tool_name, tool_use_id, is_sidechain, is_meta, payload, parser_version) \
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind("ev_000")
    .bind("raw_000")
    .bind("observed_event.v1")
    .bind(sess)
    .bind(ts)
    .bind("assistant")
    .bind("tool_call")
    .bind("Bash")
    .bind("tu_000")
    .bind(0i64)
    .bind(0i64)
    .bind(&call_payload)
    .bind("v1")
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_risky_action_negative(pool: &sqlx::SqlitePool, sess: &str) {
    let ts = "2026-01-01T00:00:00Z";
    insert_raw(pool, "raw_000", ts).await;

    // tool_call: safe ls command
    let call_payload = serde_json::to_string(&serde_json::json!({
        "tool_use": {
            "name": "Bash",
            "input": { "command": "ls -la" }
        }
    }))
    .unwrap();
    sqlx::query(
        "INSERT OR IGNORE INTO observed_event \
         (event_id, raw_event_id, schema_version, session_id, observed_at, \
          actor, kind, tool_name, tool_use_id, is_sidechain, is_meta, payload, parser_version) \
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind("ev_000")
    .bind("raw_000")
    .bind("observed_event.v1")
    .bind(sess)
    .bind(ts)
    .bind("assistant")
    .bind("tool_call")
    .bind("Bash")
    .bind("tu_000")
    .bind(0i64)
    .bind(0i64)
    .bind(&call_payload)
    .bind("v1")
    .execute(pool)
    .await
    .unwrap();
}

// ---------------------------------------------------------------------------
// context_bloat positive: large tool_result + short assistant message
// ---------------------------------------------------------------------------

async fn seed_context_bloat_positive(pool: &sqlx::SqlitePool, sess: &str) {
    let ts0 = "2026-01-01T00:00:00Z";
    let ts1 = "2026-01-01T00:00:01Z";
    insert_raw(pool, "raw_000", ts0).await;
    insert_raw(pool, "raw_001", ts1).await;

    let big_content = "Z".repeat(60_000); // > 50KB
    let result_payload = serde_json::to_string(&serde_json::json!({
        "tool_result": {
            "tool_use_id": "tu_000",
            "content": big_content,
            "is_error": false
        }
    }))
    .unwrap();

    sqlx::query(
        "INSERT OR IGNORE INTO observed_event \
         (event_id, raw_event_id, schema_version, session_id, observed_at, \
          actor, kind, tool_name, tool_use_id, is_sidechain, is_meta, payload, parser_version) \
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind("ev_000")
    .bind("raw_000")
    .bind("observed_event.v1")
    .bind(sess)
    .bind(ts0)
    .bind("tool")
    .bind("tool_result")
    .bind("Grep")
    .bind("tu_000")
    .bind(0i64)
    .bind(0i64)
    .bind(&result_payload)
    .bind("v1")
    .execute(pool)
    .await
    .unwrap();

    let asst_payload = serde_json::to_string(&serde_json::json!({
        "message": { "content": [{"type":"text","text":"ok"}] }
    }))
    .unwrap();
    sqlx::query(
        "INSERT OR IGNORE INTO observed_event \
         (event_id, raw_event_id, schema_version, session_id, observed_at, \
          actor, kind, is_sidechain, is_meta, payload, parser_version) \
         VALUES (?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind("ev_001")
    .bind("raw_001")
    .bind("observed_event.v1")
    .bind(sess)
    .bind(ts1)
    .bind("assistant")
    .bind("assistant_message")
    .bind(0i64)
    .bind(0i64)
    .bind(&asst_payload)
    .bind("v1")
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_context_bloat_negative(pool: &sqlx::SqlitePool, sess: &str) {
    let ts = "2026-01-01T00:00:00Z";
    insert_raw(pool, "raw_000", ts).await;

    // Small result — below threshold
    let result_payload = serde_json::to_string(&serde_json::json!({
        "tool_result": {
            "tool_use_id": "tu_000",
            "content": "small output",
            "is_error": false
        }
    }))
    .unwrap();
    sqlx::query(
        "INSERT OR IGNORE INTO observed_event \
         (event_id, raw_event_id, schema_version, session_id, observed_at, \
          actor, kind, tool_name, tool_use_id, is_sidechain, is_meta, payload, parser_version) \
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind("ev_000")
    .bind("raw_000")
    .bind("observed_event.v1")
    .bind(sess)
    .bind(ts)
    .bind("tool")
    .bind("tool_result")
    .bind("Bash")
    .bind("tu_000")
    .bind(0i64)
    .bind(0i64)
    .bind(&result_payload)
    .bind("v1")
    .execute(pool)
    .await
    .unwrap();
}

// ---------------------------------------------------------------------------
// final_state_mismatch positive: goal verb + failed verification
// ---------------------------------------------------------------------------

async fn seed_final_state_mismatch_positive(pool: &sqlx::SqlitePool, sess: &str) {
    let ts0 = "2026-01-01T00:00:00Z";
    let ts1 = "2026-01-01T00:00:01Z";
    insert_raw(pool, "raw_000", ts0).await;
    insert_raw(pool, "raw_001", ts1).await;
    insert_raw(pool, "raw_002", "2026-01-01T00:00:02Z").await;

    let user_payload = serde_json::to_string(&serde_json::json!({
        "message": { "content": [{"type":"text","text":"fix the failing tests in the auth module"}] }
    }))
    .unwrap();
    sqlx::query(
        "INSERT OR IGNORE INTO observed_event \
         (event_id, raw_event_id, schema_version, session_id, observed_at, \
          actor, kind, is_sidechain, is_meta, payload, parser_version) \
         VALUES (?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind("ev_000")
    .bind("raw_000")
    .bind("observed_event.v1")
    .bind(sess)
    .bind(ts0)
    .bind("user")
    .bind("user_message")
    .bind(0i64)
    .bind(0i64)
    .bind(&user_payload)
    .bind("v1")
    .execute(pool)
    .await
    .unwrap();

    let bash_payload = serde_json::to_string(&serde_json::json!({
        "tool_use": { "name": "Bash", "input": { "command": "cargo test" } }
    }))
    .unwrap();
    sqlx::query(
        "INSERT OR IGNORE INTO observed_event \
         (event_id, raw_event_id, schema_version, session_id, observed_at, \
          actor, kind, tool_name, tool_use_id, is_sidechain, is_meta, payload, parser_version) \
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind("ev_001")
    .bind("raw_001")
    .bind("observed_event.v1")
    .bind(sess)
    .bind(ts1)
    .bind("assistant")
    .bind("tool_call")
    .bind("Bash")
    .bind("tu_001")
    .bind(0i64)
    .bind(0i64)
    .bind(&bash_payload)
    .bind("v1")
    .execute(pool)
    .await
    .unwrap();

    let asst_payload = serde_json::to_string(&serde_json::json!({
        "message": { "content": [{"type":"text","text":"Tests are still failing."}] }
    }))
    .unwrap();
    sqlx::query(
        "INSERT OR IGNORE INTO observed_event \
         (event_id, raw_event_id, schema_version, session_id, observed_at, \
          actor, kind, is_sidechain, is_meta, payload, parser_version) \
         VALUES (?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind("ev_002")
    .bind("raw_002")
    .bind("observed_event.v1")
    .bind(sess)
    .bind("2026-01-01T00:00:02Z")
    .bind("assistant")
    .bind("assistant_message")
    .bind(0i64)
    .bind(0i64)
    .bind(&asst_payload)
    .bind("v1")
    .execute(pool)
    .await
    .unwrap();

    // Insert a failed verification_run
    let vr = VerificationRunRow {
        verification_run_id: "vr_001".into(),
        schema_version: "verification_run.v1".into(),
        session_id: sess.into(),
        source: "bash".into(),
        command: "cargo test".into(),
        command_kind: "test".into(),
        trigger_event_id: "ev_001".into(),
        trigger_tool_use_id: Some("tu_001".into()),
        status: "failed".into(),
        detection_basis: "known_tool".into(),
        status_basis: "exit".into(),
        started_at: ts1.into(),
        ended_at: Some("2026-01-01T00:00:02Z".into()),
        exit_code: Some(1),
        failure_summary: Some("2 tests failed".into()),
        raw_event_id: "raw_001".into(),
        parser_version: "v1".into(),
    };
    witmcc::db::repo_verification_run::insert(pool, &vr)
        .await
        .unwrap();
}

async fn seed_final_state_mismatch_negative(pool: &sqlx::SqlitePool, sess: &str) {
    let ts = "2026-01-01T00:00:00Z";
    insert_raw(pool, "raw_000", ts).await;
    insert_raw(pool, "raw_001", "2026-01-01T00:00:01Z").await;

    let user_payload = serde_json::to_string(&serde_json::json!({
        "message": { "content": [{"type":"text","text":"what does this function do?"}] }
    }))
    .unwrap();
    sqlx::query(
        "INSERT OR IGNORE INTO observed_event \
         (event_id, raw_event_id, schema_version, session_id, observed_at, \
          actor, kind, is_sidechain, is_meta, payload, parser_version) \
         VALUES (?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind("ev_000")
    .bind("raw_000")
    .bind("observed_event.v1")
    .bind(sess)
    .bind(ts)
    .bind("user")
    .bind("user_message")
    .bind(0i64)
    .bind(0i64)
    .bind(&user_payload)
    .bind("v1")
    .execute(pool)
    .await
    .unwrap();

    let asst_payload = serde_json::to_string(&serde_json::json!({
        "message": { "content": [{"type":"text","text":"This function calculates the sum."}] }
    }))
    .unwrap();
    sqlx::query(
        "INSERT OR IGNORE INTO observed_event \
         (event_id, raw_event_id, schema_version, session_id, observed_at, \
          actor, kind, is_sidechain, is_meta, payload, parser_version) \
         VALUES (?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind("ev_001")
    .bind("raw_001")
    .bind("observed_event.v1")
    .bind(sess)
    .bind("2026-01-01T00:00:01Z")
    .bind("assistant")
    .bind("assistant_message")
    .bind(0i64)
    .bind(0i64)
    .bind(&asst_payload)
    .bind("v1")
    .execute(pool)
    .await
    .unwrap();
    // No verification run, no goal verb → no fire
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn build_fixture_runtime() -> JudgeRuntime {
    JudgeRuntime::fixture(std::path::Path::new(FIXTURE_JUDGE_PATH), 20).unwrap()
}

// ---------------------------------------------------------------------------
// Tests — positive (judge promotes)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn risky_action_positive_e2e() {
    let pool = make_pool().await;
    let sess = "sess_ra_pos";
    seed_risky_action_positive(&pool, sess).await;

    let runtime = build_fixture_runtime();
    run_extractors_with_runtime(&pool, sess, &runtime)
        .await
        .unwrap();

    let n: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM finding WHERE session_id=? AND category=?")
            .bind(sess)
            .bind("risky_action")
            .fetch_one(&pool)
            .await
            .unwrap();
    // FixtureJudge must have an entry for the evidence hash → 1 finding promoted
    assert_eq!(n, 1, "risky_action positive: expected 1 finding, got {n}");
}

#[tokio::test]
async fn risky_action_negative_e2e() {
    let pool = make_pool().await;
    let sess = "sess_ra_neg";
    seed_risky_action_negative(&pool, sess).await;

    let runtime = build_fixture_runtime();
    run_extractors_with_runtime(&pool, sess, &runtime)
        .await
        .unwrap();

    let n: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM finding WHERE session_id=? AND category=?")
            .bind(sess)
            .bind("risky_action")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(n, 0, "risky_action negative: expected 0 findings, got {n}");
}

#[tokio::test]
async fn context_bloat_positive_e2e() {
    let pool = make_pool().await;
    let sess = "sess_cb_pos";
    seed_context_bloat_positive(&pool, sess).await;

    let runtime = build_fixture_runtime();
    run_extractors_with_runtime(&pool, sess, &runtime)
        .await
        .unwrap();

    let n: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM finding WHERE session_id=? AND category=?")
            .bind(sess)
            .bind("context_bloat")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(n, 1, "context_bloat positive: expected 1 finding, got {n}");
}

#[tokio::test]
async fn context_bloat_negative_e2e() {
    let pool = make_pool().await;
    let sess = "sess_cb_neg";
    seed_context_bloat_negative(&pool, sess).await;

    let runtime = build_fixture_runtime();
    run_extractors_with_runtime(&pool, sess, &runtime)
        .await
        .unwrap();

    let n: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM finding WHERE session_id=? AND category=?")
            .bind(sess)
            .bind("context_bloat")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(n, 0, "context_bloat negative: expected 0 findings, got {n}");
}

#[tokio::test]
async fn final_state_mismatch_positive_e2e() {
    let pool = make_pool().await;
    let sess = "sess_fsm_pos";
    seed_final_state_mismatch_positive(&pool, sess).await;

    let runtime = build_fixture_runtime();
    run_extractors_with_runtime(&pool, sess, &runtime)
        .await
        .unwrap();

    let n: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM finding WHERE session_id=? AND category=?")
            .bind(sess)
            .bind("final_state_mismatch")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(n, 1, "final_state_mismatch positive: expected 1 finding, got {n}");
}

#[tokio::test]
async fn final_state_mismatch_negative_e2e() {
    let pool = make_pool().await;
    let sess = "sess_fsm_neg";
    seed_final_state_mismatch_negative(&pool, sess).await;

    let runtime = build_fixture_runtime();
    run_extractors_with_runtime(&pool, sess, &runtime)
        .await
        .unwrap();

    let n: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM finding WHERE session_id=? AND category=?")
            .bind(sess)
            .bind("final_state_mismatch")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(n, 0, "final_state_mismatch negative: expected 0 findings, got {n}");
}

// ---------------------------------------------------------------------------
// L1 categories unaffected: risky_action being queued must not block L1 findings
// ---------------------------------------------------------------------------

#[tokio::test]
async fn l1_categories_still_work_alongside_l2_extractors() {
    // Seed a session with a tool_failure (L1) AND a destructive Bash (L2)
    let pool = make_pool().await;
    let sess = "sess_mixed";
    let ts0 = "2026-01-01T00:00:00Z";
    let ts1 = "2026-01-01T00:00:01Z";
    insert_raw(&pool, "raw_000", ts0).await;
    insert_raw(&pool, "raw_001", ts1).await;

    // tool_result with is_error=true (triggers tool_failure L1)
    let err_payload = serde_json::to_string(&serde_json::json!({
        "tool_result": {
            "tool_use_id": "tu_err",
            "is_error": true,
            "content": "FAILED"
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
    .bind(sess)
    .bind(ts0)
    .bind("tool")
    .bind("tool_result")
    .bind("tu_err")
    .bind(0i64)
    .bind(0i64)
    .bind(&err_payload)
    .bind("v1")
    .execute(&pool)
    .await
    .unwrap();

    // tool_call with rm -rf (triggers risky_action L2)
    let rm_payload = serde_json::to_string(&serde_json::json!({
        "tool_use": { "name": "Bash", "input": { "command": "rm -rf /tmp/junk" } }
    }))
    .unwrap();
    sqlx::query(
        "INSERT OR IGNORE INTO observed_event \
         (event_id, raw_event_id, schema_version, session_id, observed_at, \
          actor, kind, tool_name, tool_use_id, is_sidechain, is_meta, payload, parser_version) \
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind("ev_001")
    .bind("raw_001")
    .bind("observed_event.v1")
    .bind(sess)
    .bind(ts1)
    .bind("assistant")
    .bind("tool_call")
    .bind("Bash")
    .bind("tu_rm")
    .bind(0i64)
    .bind(0i64)
    .bind(&rm_payload)
    .bind("v1")
    .execute(&pool)
    .await
    .unwrap();

    let runtime = build_fixture_runtime();
    run_extractors_with_runtime(&pool, sess, &runtime)
        .await
        .unwrap();

    // L1 tool_failure must have produced a finding
    let tf: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM finding WHERE session_id=? AND category=?")
            .bind(sess)
            .bind("tool_failure")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(tf >= 1, "tool_failure L1 must still fire alongside L2 extractors; got {tf}");
}

// ---------------------------------------------------------------------------
// --judge none: L2 categories produce pending, not findings
// ---------------------------------------------------------------------------

#[tokio::test]
async fn l2_categories_dormant_with_noop_judge() {
    let pool = make_pool().await;
    let sess = "sess_noop";
    seed_risky_action_positive(&pool, sess).await;

    // With noop judge, risky_action should queue to pending (not promoted)
    let runtime = JudgeRuntime::noop();
    run_extractors_with_runtime(&pool, sess, &runtime)
        .await
        .unwrap();

    let findings: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM finding WHERE session_id=? AND category=?")
            .bind(sess)
            .bind("risky_action")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        findings, 0,
        "risky_action must not produce findings with noop judge; got {findings}"
    );

    let pending: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM findings_pending_judge WHERE session_id=?")
            .bind(sess)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        pending >= 1,
        "risky_action must be in pending queue with noop judge; got {pending}"
    );
}
