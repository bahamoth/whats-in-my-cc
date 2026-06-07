//! Integration tests for the signal detector pipeline (Plan 1: finding →
//! signal). Tests that `run_detectors` writes `signal` rows and that re-running
//! is idempotent (deduplication via `signal_id`).
//!
//! We disable foreign_keys for the test pool since we directly INSERT synthetic
//! raw_event/observed_event rows without all required FK columns. This is
//! intentional: the pipeline tests verify signal generation, not ingest FK
//! integrity (which is covered by ingest_store.rs).

use sqlx::sqlite::SqlitePoolOptions;
use wimcc::db::{migrate, repo_diff_hunk};
use wimcc::db::repo_diff_hunk::NewDiffHunk;

async fn seeded_pool_with_failing_session() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();

    // Disable FK enforcement so we can insert test rows without full FK chain.
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&pool)
        .await
        .unwrap();

    migrate(&pool).await.unwrap();

    let sess = "sess_t";
    let ev_id = |i: usize| format!("ev_{i:03}");
    let raw_id = |i: usize| format!("raw_{i:03}");
    let ts = |i: usize| format!("2026-01-01T00:00:{i:02}Z");

    // Minimal raw_event rows (FK check disabled).
    for i in 0..3usize {
        sqlx::query(
            "INSERT OR IGNORE INTO raw_event \
             (raw_event_id, ingest_run_id, source_type, source_uri, source_line_no, \
              source_byte_offset, payload_sha256, payload, captured_at) \
             VALUES (?,?,?,?,?,?,?,?,?)"
        )
        .bind(raw_id(i))
        .bind("run_0")
        .bind("claude_transcript")
        .bind("test.jsonl")
        .bind(i as i64)
        .bind(0i64)
        .bind(format!("sha256_{i}"))
        .bind(b"{}" as &[u8])
        .bind(ts(i))
        .execute(&pool).await.unwrap();
    }

    // observed_event rows.
    for (i, actor, kind) in [
        (0usize, "user", "user_message"),
        (1, "assistant", "tool_call"),
        (2, "tool", "tool_result"),
    ] {
        let payload = if kind == "tool_result" {
            // Nested shape matching the real-fixture structure the tool_failure
            // extractor reads. Plan 6: ToolFailure now fires on
            // resolve_outcome==Failed (NOT is_error alone). The content carries an
            // explicit "exit code: 1" so the structural-parse tier (Tier-3) of
            // resolve_outcome returns Failed/Measured. is_error=true is retained as
            // a tool-execution fact but is no longer the trigger.
            r#"{"tool_result":{"tool_use_id":"tid_0","is_error":true,"content":"FAILED\nexit code: 1"}}"#.to_string()
        } else if kind == "tool_call" {
            r#"{"tool_use_id":"tid_0","name":"Bash","input":{"command":"cargo test"}}"#.to_string()
        } else {
            "{}".to_string()
        };

        let q = if kind == "tool_call" {
            sqlx::query(
                "INSERT OR IGNORE INTO observed_event \
                 (event_id, raw_event_id, schema_version, session_id, observed_at, \
                  actor, kind, tool_name, tool_use_id, parser_version, payload) \
                 VALUES (?,?,?,?,?,?,?,?,?,?,?)"
            )
            .bind(ev_id(i)).bind(raw_id(i))
            .bind("observed_event.v1").bind(sess)
            .bind(ts(i))
            .bind(actor).bind(kind).bind("Bash").bind("tid_0")
            .bind("test").bind(&payload)
            .execute(&pool).await.unwrap()
        } else if kind == "tool_result" {
            sqlx::query(
                "INSERT OR IGNORE INTO observed_event \
                 (event_id, raw_event_id, schema_version, session_id, observed_at, \
                  actor, kind, tool_use_id, parser_version, payload) \
                 VALUES (?,?,?,?,?,?,?,?,?,?)"
            )
            .bind(ev_id(i)).bind(raw_id(i))
            .bind("observed_event.v1").bind(sess)
            .bind(ts(i))
            .bind(actor).bind(kind).bind("tid_0")
            .bind("test").bind(&payload)
            .execute(&pool).await.unwrap()
        } else {
            sqlx::query(
                "INSERT OR IGNORE INTO observed_event \
                 (event_id, raw_event_id, schema_version, session_id, observed_at, \
                  actor, kind, parser_version, payload) \
                 VALUES (?,?,?,?,?,?,?,?,?)"
            )
            .bind(ev_id(i)).bind(raw_id(i))
            .bind("observed_event.v1").bind(sess)
            .bind(ts(i))
            .bind(actor).bind(kind)
            .bind("test").bind(&payload)
            .execute(&pool).await.unwrap()
        };
        let _ = q;
    }

    // diff_hunk introduced by the tool_call event (ev_001).
    repo_diff_hunk::insert(&pool, &NewDiffHunk {
        diff_hunk_id: "dh_001".into(),
        schema_version: "diff_hunk.v1".into(),
        session_id: sess.into(),
        file_path: "src/foo.rs".into(),
        change_type: "modify".into(),
        line_range_after_start: Some(1),
        line_range_after_end: Some(5),
        introduced_by_event_id: ev_id(1),
        introduced_by_tool_use_id: Some("tid_0".into()),
        patch_preview: "+test_line".into(),
        lines_added: 1,
        lines_removed: 0,
        user_modified: false,
    }).await.unwrap();

    pool
}

#[tokio::test]
async fn pipeline_writes_signal_rows() {
    let pool = seeded_pool_with_failing_session().await;
    let signals = wimcc::insight::pipeline::run_detectors(&pool, "sess_t")
        .await
        .unwrap();
    assert!(
        !signals.is_empty(),
        "pipeline must write at least one signal row; got 0"
    );
}

#[tokio::test]
async fn pipeline_dedupes_via_signal_id() {
    let pool = seeded_pool_with_failing_session().await;

    wimcc::insight::pipeline::run_detectors(&pool, "sess_t").await.unwrap();
    let count_before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM signal")
        .fetch_one(&pool).await.unwrap();

    wimcc::insight::pipeline::run_detectors(&pool, "sess_t").await.unwrap();
    let count_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM signal")
        .fetch_one(&pool).await.unwrap();

    assert_eq!(
        count_before, count_after,
        "re-running pipeline must not create duplicate signal rows (INSERT OR REPLACE)"
    );
}

#[tokio::test]
async fn pipeline_signals_carry_facts_not_severity() {
    let pool = seeded_pool_with_failing_session().await;
    wimcc::insight::pipeline::run_detectors(&pool, "sess_t").await.unwrap();
    // The signal schema has no severity/confidence columns: facts is the only
    // detector-specific projection. Assert every row has non-empty facts JSON.
    let bad: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM signal WHERE facts IS NULL OR facts = '' OR evidence_refs = '[]'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(bad, 0, "every signal must carry facts + non-empty evidence_refs");
}
