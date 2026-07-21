//! UX 개선 2026-06-13 — 렌더된 이벤트의 `agent_id` 노출 (events 응답).
//!
//! observed_event.agent_id(migration 0023, 원천: subagent jsonl의 top-level
//! `agentId` — `tests/agent_id_backfill.rs` 참조)는 DB·모델에는 있었지만 events
//! DTO에 실리지 않아 UI가 병렬 서브에이전트를 구분할 수 없었다. tool_use_id 등
//! 다른 correlation 키와 같은 자리(top-level 필드)로 노출한다.

use serde_json::{json, Value};
use sqlx::sqlite::SqlitePoolOptions;
use wimcc::api::AppState;
use wimcc::db::{migrate, repo_observed, repo_raw, repo_runs};
use wimcc::model::observed::{Actor, EventKind, ObservedEvent};

async fn test_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    pool
}

async fn seed(
    pool: &sqlx::SqlitePool,
    run_id: &str,
    sid: &str,
    eid: &str,
    agent_id: Option<&str>,
    is_sidechain: bool,
) {
    let raw_id = format!("raw_{eid}");
    repo_raw::insert_dedup(
        pool,
        &repo_raw::NewRaw {
            raw_event_id: raw_id.clone(),
            ingest_run_id: run_id.into(),
            source_type: "claude_transcript".into(),
            source_uri: "/tmp/agent.jsonl".into(),
            source_line_no: 0,
            source_byte_offset: 0,
            payload_sha256: format!("sha_{eid}"),
            payload: b"{}".to_vec(),
            parse_error: None,
            captured_at: chrono::Utc::now(),
            redaction_state: "not_applicable".into(),
            redaction_manifest: None,
        },
    )
    .await
    .unwrap();
    let e = ObservedEvent {
        event_id: eid.into(),
        raw_event_id: raw_id,
        schema_version: "observed_event.v1".into(),
        session_id: sid.into(),
        observed_at: chrono::Utc::now(),
        actor: Actor::Assistant,
        kind: EventKind::AssistantMessage,
        is_sidechain,
        agent_id: agent_id.map(String::from),
        payload: json!({"text": "hi", "model": "claude-opus-4-8"}),
        parser_version: "test@v0".into(),
        ..Default::default()
    };
    repo_observed::insert(pool, &e).await.unwrap();
}

#[tokio::test]
async fn events_dto_carries_agent_id_and_null_when_absent() {
    let pool = test_pool().await;
    let run = repo_runs::start(&pool).await.unwrap();
    let sid = "sess_agent_api";
    seed(&pool, &run, sid, "sc1", Some("agentX"), true).await;
    seed(&pool, &run, sid, "main1", None, false).await;

    let server =
        axum_test::TestServer::new(wimcc::api::router(AppState::new_for_tests(pool))).unwrap();
    let r = server.get(&format!("/v1/sessions/{sid}/events")).await;
    r.assert_status_ok();
    let body: Value = r.json();
    let events = body["data"]["events"].as_array().unwrap();
    let by_id = |id: &str| -> Value {
        events
            .iter()
            .find(|e| e["event_id"] == id)
            .unwrap_or_else(|| panic!("missing event {id}"))
            .clone()
    };
    let sc = by_id("sc1");
    assert_eq!(sc["agent_id"], "agentX", "sidechain event carries agent_id");
    let main = by_id("main1");
    // 이 wire의 기존 관례: NULL TEXT 컬럼은 row 매핑(try_get().ok())에서 ""로
    // 디코드되어 다른 nullable 문자열 필드(subkind 등)와 동일하게 빈 문자열로
    // 렌더된다. 소비자는 ''/null 둘 다 "없음"으로 다뤄야 한다.
    let absent = &main["agent_id"];
    assert!(
        absent.is_null() || absent == "",
        "main-chain event has no agent attribution (got {absent})"
    );
}
