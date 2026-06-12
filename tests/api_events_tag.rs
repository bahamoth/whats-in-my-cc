//! Task 12 — 렌더된 이벤트의 `tag` 노출 (events 응답).
//!
//! UI와 MCP 소비자가 같은 태그 어휘를 보도록, tool_call 이벤트는
//! `tag {value, disposition, token, display}`를 싣고 그 외 kind는 null이다.

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
    kind: EventKind,
    tool_name: Option<&str>,
    payload: serde_json::Value,
) {
    let raw_id = format!("raw_{eid}");
    repo_raw::insert_dedup(
        pool,
        &repo_raw::NewRaw {
            raw_event_id: raw_id.clone(),
            ingest_run_id: run_id.into(),
            source_type: "claude_transcript".into(),
            source_uri: "/tmp/tag.jsonl".into(),
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
        kind,
        tool_name: tool_name.map(String::from),
        payload,
        parser_version: "test@v0".into(),
        ..Default::default()
    };
    repo_observed::insert(pool, &e).await.unwrap();
}

#[tokio::test]
async fn tool_call_events_carry_tag_and_others_null() {
    let pool = test_pool().await;
    let run = repo_runs::start(&pool).await.unwrap();
    let sid = "sess_tag_api";
    seed(
        &pool,
        &run,
        sid,
        "tc1",
        EventKind::ToolCall,
        Some("Bash"),
        json!({"input": {"command": "cd /repo && grep -n foo src"}}),
    )
    .await;
    seed(
        &pool,
        &run,
        sid,
        "um1",
        EventKind::UserMessage,
        None,
        json!({"text": "hello"}),
    )
    .await;

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
    let tc = by_id("tc1");
    assert_eq!(tc["tag"]["value"], "read.file");
    assert_eq!(tc["tag"]["disposition"], "tagged");
    assert_eq!(tc["tag"]["token"], "grep");
    assert_eq!(tc["tag"]["display"], "grep -n foo src");
    let um = by_id("um1");
    assert!(um["tag"].is_null(), "non-tool_call must have tag: null");
}
