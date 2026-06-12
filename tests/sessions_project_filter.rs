//! Dogfood 2026-06-12 — `GET /v1/sessions?project=<path>` (retrospect §3-3).
//!
//! The session-retrospect skill runs from a project root and must find "the
//! sessions that ran in this project" without the user hand-copying session
//! IDs. `cwd` is already stored per observed_event (transcript `cwd` field),
//! so the filter is: sessions having ≥1 event whose cwd equals the given path
//! (trailing slash normalised). Also exposed through the MCP
//! `search_sessions` tool (`project` argument).

use axum_test::TestServer;
use chrono::{TimeZone, Utc};
use serde_json::Value;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;

use wimcc::db::{migrate, repo_observed, repo_raw, repo_runs};
use wimcc::model::observed::{Actor, EventKind, ObservedEvent};

async fn seed_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    let run_id = repo_runs::start(&pool).await.unwrap();
    let base = Utc.with_ymd_and_hms(2026, 6, 12, 0, 0, 0).unwrap();
    // session-a ran in /Users/u/projects/alpha, session-b in /Users/u/projects/beta
    let seeds = [
        ("sess-a", "/Users/u/projects/alpha"),
        ("sess-b", "/Users/u/projects/beta"),
    ];
    let mut i = 0;
    for (sess, cwd) in seeds {
        for _ in 0..3 {
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
                event_id: format!("01P{i:023}"),
                raw_event_id: raw_id,
                schema_version: "0.5.0".into(),
                session_id: sess.into(),
                observed_at: base + chrono::Duration::seconds(i as i64),
                actor: Actor::User,
                kind: EventKind::UserMessage,
                parser_version: "test".into(),
                cwd: Some(cwd.into()),
                ..Default::default()
            };
            repo_observed::insert(&pool, &ev).await.unwrap();
            i += 1;
        }
    }
    pool
}

async fn setup() -> TestServer {
    let pool = seed_pool().await;
    let app = wimcc::api::router(wimcc::api::AppState::new_for_tests(pool));
    TestServer::new(app).unwrap()
}

#[tokio::test]
async fn project_filter_returns_only_matching_sessions() {
    let s = setup().await;
    let v: Value = s
        .get("/v1/sessions?project=/Users/u/projects/alpha")
        .await
        .json();
    let data = v["data"].as_array().unwrap();
    assert_eq!(data.len(), 1);
    assert_eq!(data[0]["session_id"], "sess-a");
}

#[tokio::test]
async fn project_filter_normalises_trailing_slash() {
    let s = setup().await;
    let v: Value = s
        .get("/v1/sessions?project=/Users/u/projects/alpha/")
        .await
        .json();
    let data = v["data"].as_array().unwrap();
    assert_eq!(data.len(), 1);
    assert_eq!(data[0]["session_id"], "sess-a");
}

#[tokio::test]
async fn project_filter_no_match_returns_empty() {
    let s = setup().await;
    let v: Value = s.get("/v1/sessions?project=/nowhere").await.json();
    assert!(v["data"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn without_project_filter_returns_all_sessions() {
    let s = setup().await;
    let v: Value = s.get("/v1/sessions").await.json();
    assert_eq!(v["data"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn mcp_search_sessions_accepts_project_argument() {
    let pool = seed_pool().await;
    let app = wimcc::api::router(wimcc::api::AppState::new_for_tests(pool));
    let server = TestServer::new(app).unwrap();
    // MCP handshake: initialize → Mcp-Session-Id header (slice-17 contract).
    let init = server
        .post("/mcp")
        .content_type("application/json")
        .json(&serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "t", "version": "0"}
            }
        }))
        .await;
    let sid = init.header("Mcp-Session-Id").to_str().unwrap().to_string();
    let resp = server
        .post("/mcp")
        .content_type("application/json")
        .add_header(
            axum::http::HeaderName::from_static("mcp-session-id"),
            axum::http::HeaderValue::from_str(&sid).unwrap(),
        )
        .json(&serde_json::json!({
            "jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": {
                "name": "whats_in_my_cc.search_sessions",
                "arguments": { "project": "/Users/u/projects/beta" }
            }
        }))
        .await;
    let v: Value = resp.json();
    assert_eq!(v["result"]["isError"], false);
    let text = v["result"]["content"][0]["text"].as_str().unwrap();
    let body: Value = serde_json::from_str(text).unwrap();
    let sessions = body["data"].as_array().unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0]["session_id"], "sess-b");
}
