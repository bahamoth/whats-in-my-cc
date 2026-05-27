# Slice-17 Implementation Plan — MCP Streamable HTTP

**Spec:** `docs/superpowers/specs/2026-05-27-witmcc-slice17-mcp-streamable-http-design.md`
**Branch:** `slice17-mcp-streamable-http`

---

## Phase 0 — Branch & baseline

| 0a | Cut `slice17-mcp-streamable-http` off slice-16 merge |
| 0b | Record cargo + vitest baselines |
| 0c | Confirm we can run the MCP Inspector (`npx @modelcontextprotocol/inspector`) as the smoke target |

---

## Phase 1 — Red-locking tests

### Task 1 — initialize handshake

**Files:** `tests/mcp_initialize.rs`.

```rust
#[tokio::test]
async fn initialize_returns_protocol_version_and_session_id() {
    let pool = test_pool().await;
    let server = axum_test::TestServer::new(witmcc::api::build_router(pool)).unwrap();
    let r = server.post("/mcp")
        .json(&serde_json::json!({
            "jsonrpc":"2.0","id":1,"method":"initialize",
            "params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"t","version":"0"}}
        }))
        .await;
    r.assert_status_ok();
    let body: serde_json::Value = r.json();
    assert_eq!(body["result"]["protocolVersion"], "2024-11-05");
    assert!(body["result"]["capabilities"]["tools"].is_object());
    assert!(body["result"]["capabilities"]["resources"]["subscribe"] == true);
    // Mcp-Session-Id header present
    let sid = r.header("Mcp-Session-Id");
    assert!(sid.to_str().unwrap().starts_with("mcps_"));
}

#[tokio::test]
async fn unknown_method_returns_jsonrpc_method_not_found() {
    let server = axum_test::TestServer::new(witmcc::api::build_router(test_pool().await)).unwrap();
    let r = server.post("/mcp")
        .json(&serde_json::json!({"jsonrpc":"2.0","id":2,"method":"does_not_exist","params":{}}))
        .await;
    let b: serde_json::Value = r.json();
    assert_eq!(b["error"]["code"], -32601);
}
```

### Task 2 — tools/list compat

**Files:** `tests/mcp_tools_list.rs`, `tests/fixtures/mcp/tools_list_expected.json`.

The expected fixture lists all six tools with locked input schemas.

```rust
#[tokio::test]
async fn tools_list_matches_compat_fixture() {
    let sid = init_session(&server).await;
    let r = server.post("/mcp")
        .add_header("Mcp-Session-Id", &sid)
        .json(&serde_json::json!({"jsonrpc":"2.0","id":3,"method":"tools/list"}))
        .await;
    let got: serde_json::Value = r.json();
    let expected: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string("tests/fixtures/mcp/tools_list_expected.json").unwrap()
    ).unwrap();
    assert_eq!(got["result"], expected, "tools/list shape diverged");
}
```

### Task 3 — tools/call basic

**Files:** `tests/mcp_tools_call.rs`.

```rust
#[tokio::test]
async fn get_session_graph_tool_returns_envelope_in_text_block() {
    let (server, sid, pool) = init_with_seeded_session().await;
    let r = server.post("/mcp")
        .add_header("Mcp-Session-Id", &sid)
        .json(&serde_json::json!({
            "jsonrpc":"2.0","id":4,"method":"tools/call",
            "params":{"name":"whats_in_my_cc.get_session_graph","arguments":{"session_id":"sess_t1"}}
        }))
        .await;
    let b: serde_json::Value = r.json();
    assert_eq!(b["result"]["isError"], false);
    let text = b["result"]["content"][0]["text"].as_str().unwrap();
    let env: serde_json::Value = serde_json::from_str(text).unwrap();
    assert!(env["data"]["nodes"].is_array());
}
```

(One test per tool, six total.)

### Task 4 — resources

**Files:** `tests/mcp_resources_list.rs`, `tests/mcp_resources_read.rs`.

```rust
#[tokio::test]
async fn resources_templates_list() {
    let (server, sid) = init().await;
    let r = server.post("/mcp")
        .add_header("Mcp-Session-Id", &sid)
        .json(&serde_json::json!({"jsonrpc":"2.0","id":5,"method":"resources/templates/list"}))
        .await;
    let b: serde_json::Value = r.json();
    let templates = b["result"]["resourceTemplates"].as_array().unwrap();
    let uris: Vec<&str> = templates.iter().map(|t| t["uriTemplate"].as_str().unwrap()).collect();
    assert!(uris.contains(&"whats-in-my-cc://sessions/{session_id}"));
    assert!(uris.contains(&"whats-in-my-cc://findings/{finding_id}"));
}

#[tokio::test]
async fn resources_read_session_returns_summary() {
    let (server, sid, _pool) = init_with_seeded_session().await;
    let r = server.post("/mcp")
        .add_header("Mcp-Session-Id", &sid)
        .json(&serde_json::json!({
            "jsonrpc":"2.0","id":6,"method":"resources/read",
            "params":{"uri":"whats-in-my-cc://sessions/sess_t1"}
        }))
        .await;
    let b: serde_json::Value = r.json();
    let content_uri = b["result"]["contents"][0]["uri"].as_str().unwrap();
    assert_eq!(content_uri, "whats-in-my-cc://sessions/sess_t1");
    let body_json: serde_json::Value = serde_json::from_str(
        b["result"]["contents"][0]["text"].as_str().unwrap()
    ).unwrap();
    assert!(body_json["data"].is_object());
}
```

### Task 5 — SSE channel + notifications

**Files:** `tests/mcp_sse.rs`.

```rust
#[tokio::test]
async fn sse_channel_emits_initialized_then_resource_updated_on_rebuild() {
    let (server, sid, pool) = init_with_seeded_session().await;

    // Open SSE in a tokio task
    let url = server.server_url("/mcp");
    let h = tokio::spawn(async move {
        // hand-rolled SSE client over reqwest; collect first two events
        client_collect_two_events(&url, &sid).await
    });

    // Trigger rebuild
    witmcc::graph::build::rebuild_session(&pool, "sess_t1").await.unwrap();

    let events = h.await.unwrap();
    assert!(events.iter().any(|e| e["method"] == "notifications/initialized"));
    assert!(events.iter().any(|e| e["method"] == "notifications/resources/updated"
                                  && e["params"]["uri"].as_str().unwrap()
                                       == "whats-in-my-cc://sessions/sess_t1/graph"));
}
```

### Task 6 — Origin validation

**Files:** `tests/mcp_origin_validation.rs`.

```rust
#[tokio::test]
async fn rejects_disallowed_origin() {
    let server = axum_test::TestServer::new(witmcc::api::build_router(test_pool().await)).unwrap();
    let r = server.post("/mcp")
        .add_header("Origin", "https://evil.example.com")
        .json(&serde_json::json!({"jsonrpc":"2.0","id":7,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}))
        .await;
    r.assert_status(http::StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn allows_localhost_origin() {
    let server = axum_test::TestServer::new(witmcc::api::build_router(test_pool().await)).unwrap();
    let r = server.post("/mcp")
        .add_header("Origin", "http://127.0.0.1:1234")
        .json(&serde_json::json!({"jsonrpc":"2.0","id":8,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}))
        .await;
    r.assert_status_ok();
}
```

**Commit 1:** `test(slice-17): red-locking tests for MCP transport + methods + compat`

---

## Phase 2 — JSON-RPC framing + skeleton

| 7  | `src/api/mcp/jsonrpc.rs` — Request, Response, Error types + (de)serialise. Batch support. |
| 8  | `src/api/mcp/transport.rs` — POST handler that dispatches into `methods::handle(request)` and serialises the Response. GET handler that opens SSE. |
| 9  | Register `/mcp` route in `src/api/mod.rs`. |
| 10 | Empty `tools/list` and `resources/list` returning `[]` so the compile passes. |

`initialize` test green.

**Commit 2:** `feat(api/mcp): JSON-RPC framing + transport skeleton`

---

## Phase 3 — Lifecycle + session state

| 11 | `Mcp-Session-Id` generation + storage in an `Arc<DashMap<String, McpSession>>` in app state |
| 12 | `notifications/initialized` handler (marks session active) |
| 13 | `unknown method ⇒ -32601` |

`initialize` test fully green; unknown method test green.

**Commit 3:** `feat(api/mcp): session lifecycle + Mcp-Session-Id`

---

## Phase 4 — Tools

| 14 | `src/api/mcp/tools/get_session_graph.rs` — delegates to existing `session_graph` handler logic, wraps the envelope into MCP `text` content. |
| 15 | `src/api/mcp/tools/search_sessions.rs` |
| 16 | `src/api/mcp/tools/search_findings.rs` |
| 17 | `src/api/mcp/tools/explain_node.rs` — combines `/v1/findings` + graph context for one node |
| 18 | `src/api/mcp/tools/get_file_lineage.rs` |
| 19 | `src/api/mcp/tools/get_otel_trace.rs` |
| 20 | `src/api/mcp/tools/mod.rs::tools_list_response()` |
| 21 | `tests/fixtures/mcp/tools_list_expected.json` (the golden) |

tools/list + tools/call tests green.

**Commit 4:** `feat(api/mcp): tools/list + tools/call + 6 tool impls`

---

## Phase 5 — Resources

| 22 | URI parser in `src/api/mcp/resources/parse.rs` — matches the six templates and extracts placeholders |
| 23 | Handler `resources/templates/list` |
| 24 | Handler `resources/list` (paginated via DB query) |
| 25 | Handler `resources/read` — dispatches to per-resource fetchers |
| 26 | `src/api/mcp/resources/sessions.rs`, `findings.rs`, `lineage.rs`, `traces.rs` |

Resources tests green.

**Commit 5:** `feat(api/mcp): resources URI scheme + list/read handlers`

---

## Phase 6 — SSE fan-out

| 27 | `tokio::sync::broadcast` channel in app state |
| 28 | GET `/mcp` handler that subscribes to the broadcast and writes SSE events |
| 29 | `rebuild_session` hook: emit `notifications/resources/updated` with the affected URI |
| 30 | `notifications/initialized` echo on first SSE event after connect |

SSE test green.

**Commit 6:** `feat(api/mcp): SSE notifications fan-out on rebuild_session`

---

## Phase 7 — Origin + compat golden

| 31 | Extend existing Host-allowlist middleware to also enforce Origin allowlist for `/mcp` routes only |
| 32 | `tests/fixtures/mcp/protocol_compat.json` (initialize + tools/list + resources/templates/list golden) |
| 33 | `tests/mcp_spec_compat.rs` |

**Commit 7:** `feat(api/mcp): Origin enforcement + protocol compat golden`

---

## Phase 8 — Smoke + verification

```
Smoke — slice-17

[ ] witmcc init-db; ingest aac68973
[ ] witmcc serve --port 4337 &
[ ] npx @modelcontextprotocol/inspector
    - Connect URL: http://127.0.0.1:4337/mcp
    - Verify initialize handshake (Mcp-Session-Id received)
    - tools/list shows six tools
    - tools/call whats_in_my_cc.get_session_graph with arguments={session_id:"aac68973"} returns JSON
    - resources/list shows session entries; resources/read whats-in-my-cc://sessions/aac68973 returns summary
[ ] curl -N -H "Mcp-Session-Id: <sid>" http://127.0.0.1:4337/mcp &
    # SSE channel
[ ] Trigger a rebuild (re-ingest aac68973)
[ ] Observe the SSE channel emits notifications/resources/updated
[ ] Curl with Origin: https://evil.example.com — confirm 403
```

```
Verification — slice-17

- cargo test count: baseline (post slice-16) → expected + 15..20
- New endpoint /mcp registered
- Protocol compat fixture present, diff-asserted
- aac68973 rebuild emits one notifications/resources/updated per connected SSE
- AC-5 now closed (Pull API + MCP both expose the same data, no write surface)
```

---

## Phase 9 — PR

Title: `feat(slice-17): MCP Streamable HTTP transport (read-only, hand-rolled)`. Implementation-notes update. CLAUDE.md status update marking M6 closed.
