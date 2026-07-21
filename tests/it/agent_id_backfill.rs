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

/// raw_event 중 parse_error 행 등은 payload가 비-JSON일 수 있다. backfill이 거기서
/// 에러를 내 전체 UPDATE를 깨뜨리면 안 된다 — malformed는 건너뛰고 valid 행만 채운다.
/// 실데이터 2026-06-11: 프로덕션 backfill이 "malformed JSON"으로 전체 실패했다.
#[tokio::test]
async fn backfill_skips_malformed_payload() {
    let pool = empty_pool().await;
    let run_id = repo_runs::start(&pool).await.unwrap();
    // malformed payload (parse_error row shape — not valid JSON)
    repo_raw::insert_dedup(
        &pool,
        &repo_raw::NewRaw {
            raw_event_id: "rbad".into(),
            ingest_run_id: run_id.clone(),
            source_type: "unparseable".into(),
            source_uri: "/t.jsonl".into(),
            source_line_no: 0,
            source_byte_offset: 0,
            payload_sha256: "bad".into(),
            payload: b"not json at all".to_vec(),
            parse_error: Some("boom".into()),
            captured_at: Utc::now(),
            redaction_state: "not_applicable".into(),
            redaction_manifest: None,
        },
    )
    .await
    .unwrap();
    let bad = ObservedEvent {
        event_id: "ebad".into(),
        raw_event_id: "rbad".into(),
        schema_version: "observed_event.v1".into(),
        session_id: "sess".into(),
        kind: EventKind::ToolCall,
        parser_version: "test".into(),
        ..Default::default()
    };
    repo_observed::insert(&pool, &bad).await.unwrap();
    // valid agentId row in the same session
    repo_raw::insert_dedup(
        &pool,
        &repo_raw::NewRaw {
            raw_event_id: "rok".into(),
            ingest_run_id: run_id,
            source_type: "claude_transcript".into(),
            source_uri: "/t.jsonl".into(),
            source_line_no: 1,
            source_byte_offset: 0,
            payload_sha256: "ok".into(),
            payload: br#"{"agentId":"agentZ"}"#.to_vec(),
            parse_error: None,
            captured_at: Utc::now(),
            redaction_state: "not_applicable".into(),
            redaction_manifest: None,
        },
    )
    .await
    .unwrap();
    let ok = ObservedEvent {
        event_id: "eok".into(),
        raw_event_id: "rok".into(),
        schema_version: "observed_event.v1".into(),
        session_id: "sess".into(),
        kind: EventKind::ToolCall,
        parser_version: "test".into(),
        ..Default::default()
    };
    repo_observed::insert(&pool, &ok).await.unwrap();

    // must NOT error on the malformed row; fills only the valid one
    let n = repo_observed::backfill_agent_id(&pool).await.unwrap();
    assert_eq!(
        n, 1,
        "only the valid-JSON row backfilled, malformed skipped"
    );
}
