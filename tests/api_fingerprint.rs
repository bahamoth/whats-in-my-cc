//! GET /v1/sessions/:id/fingerprint — envelope + 결정론 관측 필드 (Task 4).

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

async fn seed_assistant(pool: &sqlx::SqlitePool, run_id: &str, sid: &str, eid: &str, model: &str) {
    let raw_id = format!("raw_{eid}");
    repo_raw::insert_dedup(
        pool,
        &repo_raw::NewRaw {
            raw_event_id: raw_id.clone(),
            ingest_run_id: run_id.into(),
            source_type: "claude_transcript".into(),
            source_uri: "/tmp/fpapi.jsonl".into(),
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
        payload: json!({"model": model}),
        cc_version: Some("2.1.0".into()),
        parser_version: "test@v0".into(),
        ..Default::default()
    };
    repo_observed::insert(pool, &e).await.unwrap();
}

#[tokio::test]
async fn fingerprint_endpoint_returns_envelope_with_models() {
    let pool = test_pool().await;
    let run = repo_runs::start(&pool).await.unwrap();
    seed_assistant(&pool, &run, "sess_fp_api", "e1", "claude-opus-4-7").await;
    let server =
        axum_test::TestServer::new(wimcc::api::router(AppState::new_for_tests(pool))).unwrap();
    let r = server.get("/v1/sessions/sess_fp_api/fingerprint").await;
    r.assert_status_ok();
    let body: Value = r.json();
    assert!(body["meta"]["schema_version"].is_string());
    let data = &body["data"];
    assert_eq!(data["session_id"], "sess_fp_api");
    assert_eq!(data["models"][0], "claude-opus-4-7");
    assert_eq!(data["cc_versions"][0], "2.1.0");
}

#[tokio::test]
async fn fingerprint_endpoint_empty_session_returns_empty_observation() {
    let pool = test_pool().await;
    let server =
        axum_test::TestServer::new(wimcc::api::router(AppState::new_for_tests(pool))).unwrap();
    let r = server.get("/v1/sessions/sess_fp_void/fingerprint").await;
    r.assert_status_ok();
    let body: Value = r.json();
    assert!(body["data"]["models"].as_array().unwrap().is_empty());
}

/// 4차 개정 — plugins(관측된 MCP server_key 집합)와 instructions(전향 관측)
/// 노출. plugins는 tool_call payload의 tool_name에서 파생, instructions는
/// instruction_observation 테이블에서 읽는다.
#[tokio::test]
async fn fingerprint_exposes_plugins_and_instructions() {
    let pool = test_pool().await;
    let run = repo_runs::start(&pool).await.unwrap();
    seed_assistant(&pool, &run, "sess_fp2", "ev_fp2_a", "claude-opus-4-8").await;
    // tool_call: plugin 접두 서버와 일반 서버 — server_key로 접힌다.
    for (eid, tool) in [
        ("ev_fp2_t1", "mcp__plugin_serena_serena__find_symbol"),
        ("ev_fp2_t2", "mcp__claude-in-chrome__navigate"),
        ("ev_fp2_t3", "mcp__plugin_serena_serena__read_file"),
    ] {
        seed_tool_call(&pool, &run, "sess_fp2", eid, tool).await;
    }
    sqlx::query(
        "INSERT INTO instruction_snapshot (content_sha256, content, first_observed_at)
         VALUES ('aabbccdd', '# CLAUDE.md', '2026-07-04T00:00:00+00:00')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO instruction_observation (observation_id, session_id, source, path, content_sha256, observed_at)
         VALUES ('obs1', 'sess_fp2', 'project', '/w/CLAUDE.md', 'aabbccdd', '2026-07-04T00:00:01+00:00')",
    )
    .execute(&pool)
    .await
    .unwrap();
    // Tier2/3(존재 기록)은 코호트 키가 아니다 — fingerprint.instructions는
    // Tier1(project/user)만 포함한다(스펙 §2 4차: 로드 무주장).
    for (oid, source, path) in [
        ("obs2", "tree", "/w/sub/CLAUDE.md"),
        ("obs3", "import", "/w/docs/rules.md"),
    ] {
        sqlx::query(
            "INSERT INTO instruction_observation (observation_id, session_id, source, path, content_sha256, observed_at)
             VALUES (?, 'sess_fp2', ?, ?, 'aabbccdd', '2026-07-04T00:00:02+00:00')",
        )
        .bind(oid)
        .bind(source)
        .bind(path)
        .execute(&pool)
        .await
        .unwrap();
    }

    let f = wimcc::insight::fingerprint::compute_session_fingerprint(&pool, "sess_fp2")
        .await
        .unwrap();
    let v: Value = serde_json::to_value(&f).unwrap();
    assert_eq!(v["plugins"], json!(["claude-in-chrome", "serena"]));
    assert_eq!(v["instructions"].as_array().unwrap().len(), 1);
    assert_eq!(v["instructions"][0]["source"], "project");
    assert_eq!(v["instructions"][0]["hash"], "aabbccdd");
}

async fn seed_tool_call(pool: &sqlx::SqlitePool, run_id: &str, sid: &str, eid: &str, tool: &str) {
    let raw_id = format!("raw_{eid}");
    repo_raw::insert_dedup(
        pool,
        &repo_raw::NewRaw {
            raw_event_id: raw_id.clone(),
            ingest_run_id: run_id.into(),
            source_type: "claude_transcript".into(),
            source_uri: "/tmp/fpapi.jsonl".into(),
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
        kind: EventKind::ToolCall,
        payload: json!({"tool_name": tool}),
        cc_version: Some("2.1.0".into()),
        parser_version: "test@v0".into(),
        ..Default::default()
    };
    repo_observed::insert(pool, &e).await.unwrap();
}
