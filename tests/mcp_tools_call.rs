//! Slice-17 — tools/call integration tests (red-locking).
//!
//! Each tool is called with minimal valid arguments and the response
//! shape is asserted. The data content is secondary to the shape.

use axum_test::TestServer;
use serde_json::{json, Value};
use sqlx::sqlite::SqlitePoolOptions;
use wimcc::db::migrate;
use wimcc::ingest::store;

async fn make_server_with_session() -> (TestServer, sqlx::SqlitePool) {
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    store::ingest_file(
        &pool,
        std::path::Path::new("tests/fixtures/transcripts/minimal_session.jsonl"),
        &wimcc::live::NoopSink,
    )
    .await
    .unwrap();

    let state = wimcc::api::AppState::new_for_tests(pool.clone());
    let server = TestServer::new(wimcc::api::router(state)).unwrap();
    (server, pool)
}

async fn init_session(server: &TestServer) -> String {
    let r = server
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
    r.header("Mcp-Session-Id").to_str().unwrap().to_string()
}

fn sid_header(sid: &str) -> (axum::http::HeaderName, axum::http::HeaderValue) {
    (
        axum::http::HeaderName::from_static("mcp-session-id"),
        axum::http::HeaderValue::from_str(sid).unwrap(),
    )
}

#[tokio::test]
async fn search_sessions_returns_data_array() {
    let (server, _pool) = make_server_with_session().await;
    let sid = init_session(&server).await;
    let (hk, hv) = sid_header(&sid);
    let r = server
        .post("/mcp")
        .content_type("application/json")
        .add_header(hk, hv)
        .json(&json!({
            "jsonrpc": "2.0", "id": 11, "method": "tools/call",
            "params": {
                "name": "whats_in_my_cc.search_sessions",
                "arguments": { "limit": 10 }
            }
        }))
        .await;
    r.assert_status_ok();
    let body: Value = r.json();
    assert_eq!(body["result"]["isError"], false);
    let text = body["result"]["content"][0]["text"].as_str().unwrap();
    let env: Value = serde_json::from_str(text).unwrap();
    assert!(env["data"].is_array(), "search_sessions data must be array");
}

#[tokio::test]
async fn get_file_lineage_returns_content_block() {
    let (server, _pool) = make_server_with_session().await;
    let sid = init_session(&server).await;
    let (hk, hv) = sid_header(&sid);
    let r = server
        .post("/mcp")
        .content_type("application/json")
        .add_header(hk, hv)
        .json(&json!({
            "jsonrpc": "2.0", "id": 14, "method": "tools/call",
            "params": {
                "name": "whats_in_my_cc.get_file_lineage",
                "arguments": { "session_id": "sess-A", "file_path": "src/main.rs" }
            }
        }))
        .await;
    r.assert_status_ok();
    let body: Value = r.json();
    assert!(body["result"]["content"].is_array());
}

#[tokio::test]
async fn get_otel_trace_returns_content_block() {
    let (server, _pool) = make_server_with_session().await;
    let sid = init_session(&server).await;
    let (hk, hv) = sid_header(&sid);
    let r = server
        .post("/mcp")
        .content_type("application/json")
        .add_header(hk, hv)
        .json(&json!({
            "jsonrpc": "2.0", "id": 15, "method": "tools/call",
            "params": {
                "name": "whats_in_my_cc.get_otel_trace",
                "arguments": { "trace_id": "0000000000000000ffffffffffffffff" }
            }
        }))
        .await;
    r.assert_status_ok();
    let body: Value = r.json();
    assert!(body["result"]["content"].is_array());
}

/// MCP 표면 완결 (2026-07-03) — get_otel_trace는 LIMIT 절단을 숨기지 않는다.
/// HTTP `/v1/metrics`의 matched_count 계약과 동일: 반환 span 수와 별개로
/// 매칭 전체 수를 노출해 소비자(LLM)가 절단 여부를 판정할 수 있어야 한다.
#[tokio::test]
async fn get_otel_trace_exposes_truncation_via_matched_count() {
    let (server, pool) = make_server_with_session().await;
    let trace_id = "ffffffffffffffff0000000000000001";

    // Seed 201 otel_span rows on one trace — the tool's LIMIT 200 truncates.
    sqlx::query(
        "INSERT OR IGNORE INTO ingest_run (run_id, started_at, status) \
         VALUES ('run_mcp_trace', datetime('now'), 'done')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO raw_event (raw_event_id, ingest_run_id, source_type, source_uri, \
         source_line_no, source_byte_offset, payload_sha256, payload, captured_at) \
         VALUES ('raw_mcp_trace', 'run_mcp_trace', 'otel_trace', 'trace.json', 0, 0, \
         'sha_mcp_trace', '{}', datetime('now'))",
    )
    .execute(&pool)
    .await
    .unwrap();
    for i in 0..201 {
        sqlx::query(
            "INSERT INTO observed_event (event_id, raw_event_id, schema_version, session_id, \
             observed_at, actor, kind, payload, parser_version, trace_id, span_id) \
             VALUES (?, 'raw_mcp_trace', 'observed_event.v1', 'sess-trace', ?, 'system', \
             'otel_span', '{}', 'test', ?, ?)",
        )
        .bind(format!("ev_span_{i:04}"))
        .bind(format!("2026-07-03T00:{:02}:{:02}Z", i / 60, i % 60))
        .bind(trace_id)
        .bind(format!("span{i:04}"))
        .execute(&pool)
        .await
        .unwrap();
    }

    let sid = init_session(&server).await;
    let (hk, hv) = sid_header(&sid);
    let r = server
        .post("/mcp")
        .content_type("application/json")
        .add_header(hk, hv)
        .json(&json!({
            "jsonrpc": "2.0", "id": 16, "method": "tools/call",
            "params": {
                "name": "whats_in_my_cc.get_otel_trace",
                "arguments": { "trace_id": trace_id }
            }
        }))
        .await;
    r.assert_status_ok();
    let body: Value = r.json();
    assert_eq!(body["result"]["isError"], false);
    let text = body["result"]["content"][0]["text"].as_str().unwrap();
    let env: Value = serde_json::from_str(text).unwrap();
    assert_eq!(
        env["spans"].as_array().unwrap().len(),
        200,
        "LIMIT 200 keeps the response bounded"
    );
    assert_eq!(
        env["matched_count"], 201,
        "matched_count must report the pre-truncation total (no silent truncation)"
    );
}

#[tokio::test]
async fn list_detectors_returns_four_manifests() {
    let (server, _pool) = make_server_with_session().await;
    let sid = init_session(&server).await;
    let (hk, hv) = sid_header(&sid);
    let r = server
        .post("/mcp")
        .content_type("application/json")
        .add_header(hk, hv)
        .json(&json!({
            "jsonrpc": "2.0", "id": 20, "method": "tools/call",
            "params": {
                "name": "whats_in_my_cc.list_detectors",
                "arguments": {}
            }
        }))
        .await;
    r.assert_status_ok();
    let body: Value = r.json();
    assert_eq!(
        body["result"]["isError"], false,
        "list_detectors must not return isError"
    );
    let text = body["result"]["content"][0]["text"].as_str().unwrap();
    let env: Value = serde_json::from_str(text).unwrap();
    let data = env["data"]
        .as_array()
        .expect("list_detectors data must be array");
    assert_eq!(
        data.len(),
        4,
        "list_detectors must return 4 manifests; got {}",
        data.len()
    );
    // Verify each manifest has id and intent.
    for m in data {
        assert!(m["id"].is_string(), "manifest.id must be a string");
        assert!(m["intent"].is_string(), "manifest.intent must be a string");
    }
}

#[tokio::test]
async fn get_project_metrics_returns_series() {
    let (server, _pool) = make_server_with_session().await;
    let sid = init_session(&server).await;
    let (hk, hv) = sid_header(&sid);
    let r = server
        .post("/mcp")
        .content_type("application/json")
        .add_header(hk, hv)
        .json(&json!({
            "jsonrpc": "2.0", "id": 21, "method": "tools/call",
            "params": {
                "name": "whats_in_my_cc.get_project_metrics",
                "arguments": { "limit": 5 }
            }
        }))
        .await;
    r.assert_status_ok();
    let body: Value = r.json();
    assert_eq!(body["result"]["isError"], false);
    let text = body["result"]["content"][0]["text"].as_str().unwrap();
    let payload: Value = serde_json::from_str(text).unwrap();
    assert!(payload["data"]["sessions"].is_array());
    assert!(payload["data"]["matched_count"].is_i64());
    // minimal_session.jsonl이 ingest되어 있으므로 row 형태도 잠근다.
    let rows = payload["data"]["sessions"].as_array().unwrap();
    assert!(!rows.is_empty(), "ingested session must appear in series");
    assert!(rows[0]["metrics"]["tool_call_total"].is_i64());
    assert!(rows[0]["fingerprint"]["models"].is_array());
}

/// MCP parity (2026-07-03) — 회고 흐름의 세션 단위 리소스 3종(metrics·signals·
/// fingerprint)은 HTTP 전용이었다. 순수 MCP 클라이언트(session-retrospect 스킬
/// 포함)가 HTTP 폴백 없이 흐름을 완주할 수 있도록 1:1 미러 툴을 잠근다.
#[tokio::test]
async fn get_session_metrics_returns_counts() {
    let (server, _pool) = make_server_with_session().await;
    let sid = init_session(&server).await;
    let (hk, hv) = sid_header(&sid);
    let r = server
        .post("/mcp")
        .content_type("application/json")
        .add_header(hk, hv)
        .json(&json!({
            "jsonrpc": "2.0", "id": 30, "method": "tools/call",
            "params": {
                "name": "whats_in_my_cc.get_session_metrics",
                "arguments": { "session_id": "sess-A" }
            }
        }))
        .await;
    r.assert_status_ok();
    let body: Value = r.json();
    assert_eq!(body["result"]["isError"], false);
    let text = body["result"]["content"][0]["text"].as_str().unwrap();
    let env: Value = serde_json::from_str(text).unwrap();
    assert!(env["data"]["tool_call_total"].is_i64());
    assert!(env["data"]["detector_firing"].is_object());
}

#[tokio::test]
async fn get_session_signals_returns_array() {
    let (server, _pool) = make_server_with_session().await;
    let sid = init_session(&server).await;
    let (hk, hv) = sid_header(&sid);
    let r = server
        .post("/mcp")
        .content_type("application/json")
        .add_header(hk, hv)
        .json(&json!({
            "jsonrpc": "2.0", "id": 31, "method": "tools/call",
            "params": {
                "name": "whats_in_my_cc.get_session_signals",
                "arguments": { "session_id": "sess-A" }
            }
        }))
        .await;
    r.assert_status_ok();
    let body: Value = r.json();
    assert_eq!(body["result"]["isError"], false);
    let text = body["result"]["content"][0]["text"].as_str().unwrap();
    let env: Value = serde_json::from_str(text).unwrap();
    let data = env["data"].as_array().expect("signals data must be array");
    // evidence_refs 없는 Signal은 존재할 수 없다 — 행이 있으면 형태를 잠근다.
    for s in data {
        assert!(s["detector"].is_string());
        assert!(!s["evidence_refs"].as_array().unwrap().is_empty());
    }
}

#[tokio::test]
async fn get_session_fingerprint_returns_observations() {
    let (server, _pool) = make_server_with_session().await;
    let sid = init_session(&server).await;
    let (hk, hv) = sid_header(&sid);
    let r = server
        .post("/mcp")
        .content_type("application/json")
        .add_header(hk, hv)
        .json(&json!({
            "jsonrpc": "2.0", "id": 32, "method": "tools/call",
            "params": {
                "name": "whats_in_my_cc.get_session_fingerprint",
                "arguments": { "session_id": "sess-A" }
            }
        }))
        .await;
    r.assert_status_ok();
    let body: Value = r.json();
    assert_eq!(body["result"]["isError"], false);
    let text = body["result"]["content"][0]["text"].as_str().unwrap();
    let env: Value = serde_json::from_str(text).unwrap();
    assert!(env["data"]["models"].is_array());
    assert!(env["data"]["cc_versions"].is_array());
    assert!(env["data"]["entrypoints"].is_array());
}

#[tokio::test]
async fn session_scoped_tools_require_session_id() {
    let (server, _pool) = make_server_with_session().await;
    let sid = init_session(&server).await;
    for (id, name) in [
        (40, "whats_in_my_cc.get_session_metrics"),
        (41, "whats_in_my_cc.get_session_signals"),
        (42, "whats_in_my_cc.get_session_fingerprint"),
    ] {
        let (hk, hv) = sid_header(&sid);
        let r = server
            .post("/mcp")
            .content_type("application/json")
            .add_header(hk, hv)
            .json(&json!({
                "jsonrpc": "2.0", "id": id, "method": "tools/call",
                "params": { "name": name, "arguments": {} }
            }))
            .await;
        r.assert_status_ok();
        let body: Value = r.json();
        assert_eq!(
            body["result"]["isError"], true,
            "{name} without session_id must be a tool error"
        );
    }
}

#[tokio::test]
async fn unknown_tool_name_returns_is_error_true() {
    let (server, _pool) = make_server_with_session().await;
    let sid = init_session(&server).await;
    let (hk, hv) = sid_header(&sid);
    let r = server
        .post("/mcp")
        .content_type("application/json")
        .add_header(hk, hv)
        .json(&json!({
            "jsonrpc": "2.0", "id": 99, "method": "tools/call",
            "params": {
                "name": "not_a_real_tool",
                "arguments": {}
            }
        }))
        .await;
    r.assert_status_ok();
    let body: Value = r.json();
    assert_eq!(body["result"]["isError"], true);
}

// ── B-3 (2026-07-04): 응답 구조·페이지네이션·events 툴·다이제스트 ──────────

/// ① structuredContent — MCP 2025-06-18 spec: 구조화 출력은
/// `structuredContent`로 싣고, 하위호환으로 같은 JSON을 text 블록에도 담는다
/// ("SHOULD also return the serialized JSON in a TextContent block").
#[tokio::test]
async fn tool_success_carries_structured_content() {
    let (server, _pool) = make_server_with_session().await;
    let sid = init_session(&server).await;
    let (hk, hv) = sid_header(&sid);
    let r = server
        .post("/mcp")
        .add_header(hk, hv)
        .content_type("application/json")
        .json(&json!({
            "jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": {"name": "whats_in_my_cc.search_sessions", "arguments": {}}
        }))
        .await;
    let body: Value = r.json();
    let result = &body["result"];
    assert!(
        result["structuredContent"].is_object(),
        "structuredContent must be present: {result}"
    );
    // text 블록과 structuredContent는 같은 JSON이다.
    let text: Value = serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(&text, &result["structuredContent"]);
}

/// ② get_session_turns 페이지네이션 — limit/offset + 절단 노출(total_count).
#[tokio::test]
async fn get_session_turns_paginates_and_exposes_total() {
    let (server, _pool) = make_server_with_session().await;
    let sid = init_session(&server).await;
    let (hk, hv) = sid_header(&sid);
    let r = server
        .post("/mcp")
        .add_header(hk, hv)
        .content_type("application/json")
        .json(&json!({
            "jsonrpc": "2.0", "id": 3, "method": "tools/call",
            "params": {"name": "whats_in_my_cc.get_session_turns",
                       "arguments": {"session_id": "sess-A", "limit": 1, "offset": 0}}
        }))
        .await;
    let body: Value = r.json();
    let data: Value =
        serde_json::from_str(body["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    let turns = data["data"]["turns"].as_array().unwrap();
    assert_eq!(turns.len(), 1, "limit=1 must return one turn: {data}");
    assert!(
        data["data"]["total_count"].as_i64().unwrap() >= 1,
        "truncation must be exposed via total_count: {data}"
    );
}

/// ③ get_session_events — 순수 MCP 클라이언트의 원문 이벤트 창 접근.
/// HTTP GET /v1/sessions/:id/events와 같은 커서 계약(prev/next_cursor).
#[tokio::test]
async fn get_session_events_returns_cursor_window() {
    let (server, _pool) = make_server_with_session().await;
    let sid = init_session(&server).await;
    let (hk, hv) = sid_header(&sid);
    let r = server
        .post("/mcp")
        .add_header(hk, hv)
        .content_type("application/json")
        .json(&json!({
            "jsonrpc": "2.0", "id": 4, "method": "tools/call",
            "params": {"name": "whats_in_my_cc.get_session_events",
                       "arguments": {"session_id": "sess-A", "limit": 2}}
        }))
        .await;
    let body: Value = r.json();
    assert_eq!(body["result"]["isError"], false, "{body}");
    let data: Value =
        serde_json::from_str(body["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    let events = data["data"]["events"].as_array().unwrap();
    assert_eq!(events.len(), 2, "limit=2 window: {data}");
    assert!(data["data"].get("prev_cursor").is_some());
    assert!(data["data"].get("next_cursor").is_some());
}

/// ④ get_session_digest — 토큰 상한이 설계된 단일 콜: 순수 집계 조합
/// (summary+metrics+fingerprint+signals 절단 목록). 판단 문장 없음.
#[tokio::test]
async fn get_session_digest_composes_aggregates_with_caps() {
    let (server, _pool) = make_server_with_session().await;
    let sid = init_session(&server).await;
    let (hk, hv) = sid_header(&sid);
    let r = server
        .post("/mcp")
        .add_header(hk, hv)
        .content_type("application/json")
        .json(&json!({
            "jsonrpc": "2.0", "id": 5, "method": "tools/call",
            "params": {"name": "whats_in_my_cc.get_session_digest",
                       "arguments": {"session_id": "sess-A"}}
        }))
        .await;
    let body: Value = r.json();
    assert_eq!(body["result"]["isError"], false, "{body}");
    let data: Value =
        serde_json::from_str(body["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    let d = &data["data"];
    assert_eq!(d["session_id"], "sess-A");
    assert!(d["summary"]["event_count"].as_i64().unwrap() >= 6, "{d}");
    assert!(d["metrics"]["tool_call_total"].is_i64(), "{d}");
    assert!(d["fingerprint"]["models"].is_array(), "{d}");
    // signals: 절단 노출 — total과 returned가 분리돼 있다.
    assert!(d["signals"]["total"].is_i64(), "{d}");
    assert!(d["signals"]["items"].is_array(), "{d}");
}
