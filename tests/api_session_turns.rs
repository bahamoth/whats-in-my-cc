//! Dogfood 2026-06-12 — `GET /v1/sessions/:id/turns` (retrospect §3-2).
//!
//! Turn-level deterministic aggregates so an LLM consumer can judge a session
//! without downloading every event: per-user-turn tool histogram, edited
//! files, and cross-turn file churn. This is exactly the aggregation the
//! 2026-06-12 dogfooding analysis had to hand-roll in Python. Also exposed as
//! the MCP tool `whats_in_my_cc.get_session_turns`.
//!
//! Judgment stays out: counts and evidence ids only — no severity, no
//! "rework" classification (that is the LLM's call).

use axum_test::TestServer;
use chrono::{DateTime, TimeZone, Utc};
use serde_json::{json, Value};
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;

use wimcc::db::{migrate, repo_observed, repo_raw, repo_runs};
use wimcc::model::observed::{Actor, EventKind, ObservedEvent};

const SESS: &str = "sess-turns";

struct Seed {
    kind: EventKind,
    turn_id: &'static str,
    tool_name: Option<&'static str>,
    payload: Value,
    is_meta: bool,
}

async fn seed_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    let run_id = repo_runs::start(&pool).await.unwrap();
    let base: DateTime<Utc> = Utc.with_ymd_and_hms(2026, 6, 12, 1, 0, 0).unwrap();

    let seeds = vec![
        // turn-1: user asks, Edit a.html, Bash, plus a telemetry row (ignored)
        Seed {
            kind: EventKind::UserMessage,
            turn_id: "turn-1",
            tool_name: None,
            payload: json!({"text": "KPI 계산기를 만들어줘"}),
            is_meta: false,
        },
        Seed {
            kind: EventKind::ToolCall,
            turn_id: "turn-1",
            tool_name: Some("Edit"),
            payload: json!({"tool_name": "Edit", "input": {"file_path": "/p/a.html", "old_string": "x", "new_string": "y"}}),
            is_meta: false,
        },
        Seed {
            kind: EventKind::ToolCall,
            turn_id: "turn-1",
            tool_name: Some("Bash"),
            payload: json!({"tool_name": "Bash", "input": {"command": "ls"}}),
            is_meta: false,
        },
        Seed {
            kind: EventKind::MetricSample,
            turn_id: "turn-1",
            tool_name: None,
            payload: json!({}),
            is_meta: false,
        },
        // turn-2: correction turn — re-edits a.html, edits b.html, Write c.md
        Seed {
            kind: EventKind::UserMessage,
            turn_id: "turn-2",
            tool_name: None,
            payload: json!({"text": "계산기가 유기적으로 동작 안 하는데?"}),
            is_meta: false,
        },
        Seed {
            kind: EventKind::ToolCall,
            turn_id: "turn-2",
            tool_name: Some("Edit"),
            payload: json!({"tool_name": "Edit", "input": {"file_path": "/p/a.html", "old_string": "y", "new_string": "z"}}),
            is_meta: false,
        },
        Seed {
            kind: EventKind::ToolCall,
            turn_id: "turn-2",
            tool_name: Some("Edit"),
            payload: json!({"tool_name": "Edit", "input": {"file_path": "/p/b.html", "old_string": "1", "new_string": "2"}}),
            is_meta: false,
        },
        Seed {
            kind: EventKind::ToolCall,
            turn_id: "turn-2",
            tool_name: Some("Write"),
            payload: json!({"tool_name": "Write", "input": {"file_path": "/p/c.md", "content": "hi"}}),
            is_meta: false,
        },
    ];

    for (i, s) in seeds.into_iter().enumerate() {
        let raw_id = format!("raw_{i:06}");
        repo_raw::insert_dedup(
            &pool,
            &repo_raw::NewRaw {
                raw_event_id: raw_id.clone(),
                ingest_run_id: run_id.clone(),
                source_type: "test".into(),
                source_uri: format!("test://{i}"),
                source_line_no: i as i64,
                source_byte_offset: 0,
                payload_sha256: format!("sha_{i:06}"),
                payload: b"{}".to_vec(),
                parse_error: None,
                captured_at: chrono::Utc::now(),
                redaction_state: "not_applicable".into(),
                redaction_manifest: None,
            },
        )
        .await
        .unwrap();
        let ev = ObservedEvent {
            event_id: format!("01T{i:023}"),
            raw_event_id: raw_id,
            schema_version: "0.5.0".into(),
            session_id: SESS.into(),
            observed_at: base + chrono::Duration::seconds(i as i64 * 10),
            actor: Actor::User,
            kind: s.kind,
            tool_name: s.tool_name.map(String::from),
            turn_id: Some(s.turn_id.into()),
            is_meta: s.is_meta,
            payload: s.payload,
            parser_version: "test".into(),
            ..Default::default()
        };
        repo_observed::insert(&pool, &ev).await.unwrap();
    }
    pool
}

async fn setup() -> (TestServer, SqlitePool) {
    let pool = seed_pool().await;
    let app = wimcc::api::router(wimcc::api::AppState::new_for_tests(pool.clone()));
    (TestServer::new(app).unwrap(), pool)
}

#[tokio::test]
async fn turns_returns_per_turn_tool_histogram() {
    let (s, _p) = setup().await;
    let v: Value = s.get(&format!("/v1/sessions/{SESS}/turns")).await.json();
    let turns = v["data"]["turns"].as_array().unwrap();
    assert_eq!(turns.len(), 2);
    // chronological order
    assert_eq!(turns[0]["turn_id"], "turn-1");
    assert_eq!(turns[1]["turn_id"], "turn-2");
    assert_eq!(turns[0]["tool_histogram"]["Edit"], 1);
    assert_eq!(turns[0]["tool_histogram"]["Bash"], 1);
    assert_eq!(turns[0]["tool_call_total"], 2);
    assert_eq!(turns[1]["tool_histogram"]["Edit"], 2);
    assert_eq!(turns[1]["tool_histogram"]["Write"], 1);
    assert_eq!(turns[1]["tool_call_total"], 3);
}

#[tokio::test]
async fn turns_carry_tag_histogram() {
    // loop-foundations 2026-06-12 — 태그 어휘 core 이전: 턴별 tag_histogram으로
    // MCP 소비자(retrospect LLM)가 raw tool 이름이 아닌 verb.object 어휘로
    // 턴의 작업 구성을 본다. count only — 판단 없음.
    let (s, _p) = setup().await;
    let v: Value = s.get(&format!("/v1/sessions/{SESS}/turns")).await.json();
    let turns = v["data"]["turns"].as_array().unwrap();
    // turn-1: Edit a.html(write.docs) + Bash ls(read.file)
    assert_eq!(turns[0]["tag_histogram"]["write.docs"], 1);
    assert_eq!(turns[0]["tag_histogram"]["read.file"], 1);
    // turn-2: Edit a.html + Edit b.html + Write c.md — 전부 write.docs
    assert_eq!(turns[1]["tag_histogram"]["write.docs"], 3);
}

#[tokio::test]
async fn turns_carries_user_message_excerpt_and_event_id() {
    let (s, _p) = setup().await;
    let v: Value = s.get(&format!("/v1/sessions/{SESS}/turns")).await.json();
    let t0 = &v["data"]["turns"][0];
    assert_eq!(t0["user_message"]["event_id"], format!("01T{:023}", 0));
    assert!(t0["user_message"]["excerpt"]
        .as_str()
        .unwrap()
        .contains("KPI"));
    assert_eq!(t0["user_message"]["is_meta"], false);
}

#[tokio::test]
async fn turns_lists_edited_files_per_turn() {
    let (s, _p) = setup().await;
    let v: Value = s.get(&format!("/v1/sessions/{SESS}/turns")).await.json();
    let files1: Vec<&str> = v["data"]["turns"][1]["files_edited"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f.as_str().unwrap())
        .collect();
    assert_eq!(files1, vec!["/p/a.html", "/p/b.html", "/p/c.md"]);
}

#[tokio::test]
async fn file_churn_counts_turns_and_edits_across_session() {
    let (s, _p) = setup().await;
    let v: Value = s.get(&format!("/v1/sessions/{SESS}/turns")).await.json();
    let churn = v["data"]["file_churn"].as_array().unwrap();
    // /p/a.html edited in 2 turns (2 edits) — the re-edit-churn raw material.
    let a = churn
        .iter()
        .find(|c| c["file_path"] == "/p/a.html")
        .unwrap();
    assert_eq!(a["turn_count"], 2);
    assert_eq!(a["edit_count"], 2);
    let b = churn
        .iter()
        .find(|c| c["file_path"] == "/p/b.html")
        .unwrap();
    assert_eq!(b["turn_count"], 1);
    assert_eq!(b["edit_count"], 1);
    // ordered by edit_count desc → a.html first
    assert_eq!(churn[0]["file_path"], "/p/a.html");
}

#[tokio::test]
async fn unknown_session_returns_empty_turns_200() {
    let (s, _p) = setup().await;
    let v: Value = s.get("/v1/sessions/no-such/turns").await.json();
    assert!(v["data"]["turns"].as_array().unwrap().is_empty());
    assert!(v["data"]["file_churn"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn mcp_get_session_turns_returns_same_rollup() {
    let (s, _p) = setup().await;
    let init = s
        .post("/mcp")
        .content_type("application/json")
        .json(&json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "t", "version": "0"}
            }
        }))
        .await;
    let sid = init.header("Mcp-Session-Id").to_str().unwrap().to_string();
    let r = s
        .post("/mcp")
        .content_type("application/json")
        .add_header(
            axum::http::HeaderName::from_static("mcp-session-id"),
            axum::http::HeaderValue::from_str(&sid).unwrap(),
        )
        .json(&json!({
            "jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": {
                "name": "whats_in_my_cc.get_session_turns",
                "arguments": { "session_id": SESS }
            }
        }))
        .await;
    let v: Value = r.json();
    assert_eq!(v["result"]["isError"], false);
    let text = v["result"]["content"][0]["text"].as_str().unwrap();
    let body: Value = serde_json::from_str(text).unwrap();
    assert_eq!(body["data"]["turns"].as_array().unwrap().len(), 2);
    assert_eq!(body["data"]["turns"][1]["tool_histogram"]["Edit"], 2);
}
