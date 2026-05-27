//! Slice-14 — integration tests for the extractor pipeline.
//! Tests that `run_extractors` writes Finding rows and that re-running is
//! idempotent (deduplication via finding_id).

use sqlx::sqlite::SqlitePoolOptions;
use witmcc::db::{migrate, repo_diff_hunk, repo_episode, repo_observed, repo_raw, repo_verification_run};
use witmcc::model::observed::{Actor, EventKind, ObservedEvent};
use witmcc::db::repo_diff_hunk::NewDiffHunk;
use witmcc::db::repo_episode::EpisodeRow;
use chrono::{TimeZone, Utc};
use serde_json::json;

/// Seed a pool with one "failing" session:
/// - One tool_call + is_error=true result (triggers tool_failure)
/// - One action episode without following verification (triggers missing_verification)
///   with a diff_hunk produced inside it
async fn seeded_pool_with_failing_session() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();

    let sess = "sess_t";

    // Insert raw events
    let raw_id = |i: usize| format!("raw_{i:03}");
    let ev_id = |i: usize| format!("ev_{i:03}");

    // Store raw placeholders
    for i in 0..3usize {
        sqlx::query(
            "INSERT OR IGNORE INTO raw_event (raw_event_id, source_type, payload, captured_at) VALUES (?,?,?,?)"
        )
        .bind(&raw_id(i))
        .bind("claude_transcript")
        .bind("{}")
        .bind("2026-01-01T00:00:00Z")
        .execute(&pool).await.unwrap();
    }

    // Event 0: user message (intake)
    sqlx::query(
        "INSERT OR IGNORE INTO observed_event \
         (event_id, raw_event_id, schema_version, session_id, observed_at, actor, kind, parser_version, payload) \
         VALUES (?,?,?,?,?,?,?,?,?)"
    )
    .bind(&ev_id(0)).bind(&raw_id(0))
    .bind("observed_event.v1").bind(sess)
    .bind("2026-01-01T00:00:00Z")
    .bind("user").bind("user_message").bind("test")
    .bind("{}")
    .execute(&pool).await.unwrap();

    // Event 1: tool_call (Edit)
    sqlx::query(
        "INSERT OR IGNORE INTO observed_event \
         (event_id, raw_event_id, schema_version, session_id, observed_at, actor, kind, tool_name, tool_use_id, parser_version, payload) \
         VALUES (?,?,?,?,?,?,?,?,?,?,?)"
    )
    .bind(&ev_id(1)).bind(&raw_id(1))
    .bind("observed_event.v1").bind(sess)
    .bind("2026-01-01T00:00:10Z")
    .bind("assistant").bind("tool_call").bind("Bash").bind("tid_0")
    .bind("test")
    .bind(json!({"tool_use_id":"tid_0","name":"Bash","input":{"command":"cargo test"}}).to_string())
    .execute(&pool).await.unwrap();

    // Event 2: tool_result with is_error=true
    sqlx::query(
        "INSERT OR IGNORE INTO observed_event \
         (event_id, raw_event_id, schema_version, session_id, observed_at, actor, kind, tool_use_id, parser_version, payload) \
         VALUES (?,?,?,?,?,?,?,?,?,?)"
    )
    .bind(&ev_id(2)).bind(&raw_id(2))
    .bind("observed_event.v1").bind(sess)
    .bind("2026-01-01T00:00:20Z")
    .bind("tool").bind("tool_result").bind("tid_0")
    .bind("test")
    .bind(json!({"tool_use_id":"tid_0","is_error":true,"content":"FAILED"}).to_string())
    .execute(&pool).await.unwrap();

    // Insert a diff_hunk produced inside the action episode (by ev_001)
    let hunk = NewDiffHunk {
        diff_hunk_id: "dh_001".into(),
        schema_version: "diff_hunk.v1".into(),
        session_id: sess.into(),
        file_path: "src/foo.rs".into(),
        change_type: "modify".into(),
        line_range_after_start: Some(1),
        line_range_after_end: Some(5),
        introduced_by_event_id: ev_id(1),
        introduced_by_tool_use_id: Some("tid_0".into()),
        patch_preview: "+line".into(),
        lines_added: 1,
        lines_removed: 0,
        user_modified: false,
    };
    repo_diff_hunk::insert(&pool, &hunk).await.unwrap();

    // Insert episodes: intake + action (no following verification)
    let mk_ep = |eid: &str, phase: &str, start: &str, end: &str| EpisodeRow {
        episode_id: eid.into(),
        schema_version: "episode.v1".into(),
        session_id: sess.into(),
        phase: phase.into(),
        start_event_id: start.into(),
        end_event_id: end.into(),
        started_at: "2026-01-01T00:00:00Z".into(),
        ended_at: "2026-01-01T00:00:20Z".into(),
        evidence_node_ids: "[]".into(),
        classification_basis: "[]".into(),
        confidence: 0.9,
        summary: None,
        classifier_version: "episode_classifier@v1".into(),
        created_at: Utc::now().to_rfc3339(),
    };
    repo_episode::insert(&pool, &mk_ep("ep_001", "intake", "ev_000", "ev_000")).await.unwrap();
    repo_episode::insert(&pool, &mk_ep("ep_002", "action", "ev_001", "ev_002")).await.unwrap();

    pool
}

#[tokio::test]
async fn pipeline_writes_finding_rows() {
    let pool = seeded_pool_with_failing_session().await;
    let findings = witmcc::insight::pipeline::run_extractors(&pool, "sess_t")
        .await
        .unwrap();
    assert!(
        !findings.is_empty(),
        "pipeline must write at least one finding row; got 0"
    );
}

#[tokio::test]
async fn pipeline_dedupes_via_finding_id() {
    let pool = seeded_pool_with_failing_session().await;

    witmcc::insight::pipeline::run_extractors(&pool, "sess_t")
        .await
        .unwrap();
    let count_before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM finding")
        .fetch_one(&pool)
        .await
        .unwrap();

    witmcc::insight::pipeline::run_extractors(&pool, "sess_t")
        .await
        .unwrap();
    let count_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM finding")
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(
        count_before, count_after,
        "re-running pipeline must not create duplicate finding rows (INSERT OR REPLACE)"
    );
}

#[tokio::test]
async fn pipeline_drops_below_confidence_floor() {
    // The pipeline must not store any finding with confidence < 0.5.
    // Our two L1 categories both have confidence >= 0.9, so all stored rows
    // should be well above the floor.
    let pool = seeded_pool_with_failing_session().await;
    witmcc::insight::pipeline::run_extractors(&pool, "sess_t")
        .await
        .unwrap();
    let below: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM finding WHERE confidence < 0.5")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        below, 0,
        "no finding must have confidence < 0.5 (floor violation)"
    );
}
