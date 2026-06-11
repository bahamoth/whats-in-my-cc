//! Dogfooding 2026-06-11: backfill observed_event.agent_id from the raw transcript
//! payload's top-level `agentId`, so existing DBs gain subagent attribution on
//! `--auto-migrate` without a full init-db. New ingests populate it via mapping.
use chrono::Utc;
use sqlx::sqlite::SqlitePoolOptions;
use wimcc::db::{migrate, repo_observed, repo_raw, repo_runs};
use wimcc::model::observed::{EventKind, ObservedEvent};

async fn empty_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    pool
}

#[tokio::test]
async fn backfill_agent_id_fills_from_raw_payload() {
    let pool = empty_pool().await;
    let run_id = repo_runs::start(&pool).await.unwrap();
    // raw line carries top-level agentId (subagent jsonl shape)
    repo_raw::insert_dedup(
        &pool,
        &repo_raw::NewRaw {
            raw_event_id: "r1".into(),
            ingest_run_id: run_id,
            source_type: "claude_transcript".into(),
            source_uri: "/t.jsonl".into(),
            source_line_no: 0,
            source_byte_offset: 0,
            payload_sha256: "sha1".into(),
            payload: br#"{"agentId":"agentX","type":"user","isSidechain":true}"#.to_vec(),
            parse_error: None,
            captured_at: Utc::now(),
            redaction_state: "not_applicable".into(),
            redaction_manifest: None,
        },
    )
    .await
    .unwrap();
    // observed row with agent_id NULL (e.g. ingested before migration 0023)
    let ev = ObservedEvent {
        event_id: "e1".into(),
        raw_event_id: "r1".into(),
        schema_version: "observed_event.v1".into(),
        session_id: "sess".into(),
        kind: EventKind::ToolCall,
        parser_version: "test".into(),
        ..Default::default()
    };
    repo_observed::insert(&pool, &ev).await.unwrap();

    let n = repo_observed::backfill_agent_id(&pool).await.unwrap();
    assert_eq!(n, 1, "one NULL-agent_id row backfilled");

    let rows = repo_observed::list_session(&pool, "sess", 10)
        .await
        .unwrap();
    assert_eq!(rows[0].agent_id.as_deref(), Some("agentX"));
}
