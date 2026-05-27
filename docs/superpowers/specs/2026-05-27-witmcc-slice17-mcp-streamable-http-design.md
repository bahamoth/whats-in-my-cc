# Slice-17 Design — MCP Streamable HTTP

**Date:** 2026-05-27
**Branch (to be cut):** `slice17-mcp-streamable-http` off slice-16 merge.
**Goal:** Implement the MCP Streamable HTTP transport per `docs/04_api_mcp_spec.html` §5. Endpoint `/mcp`. JSON-RPC over POST; SSE on GET. Resources backed by the existing Pull API data layer. Read-only tools: `get_session_graph`, `search_findings`, `explain_node`, `get_file_lineage`, `get_otel_trace`, `search_sessions`.

This closes AC-5 (read-only integration via Pull API + MCP).

---

## 1. Motivation

The product's external surface is split into Pull API (HTTP REST for humans + scripts) and MCP Streamable HTTP (for LLM clients). Pull API is done; MCP is missing. The Insight engine work in slice-14/15/16 is most valuable when consumable by an LLM client (e.g., Claude running in a separate terminal asking "what risky actions have I taken today?"), so MCP is the natural next surface.

We deliberately **do not** depend on a Rust MCP SDK. The protocol surface we need is narrow (initialize, tools/list, tools/call, resources/list, resources/read, notifications/initialized) and stable (we pin to `protocolVersion: 2024-11-05`). Hand-implementing on top of `axum` (already in the dependency tree) saves a dep + lets us share Origin and (slice-19) auth middleware with Pull API.

---

## 2. Scope

### In scope

- New module `src/api/mcp/` with:
  - `transport.rs` — POST + SSE handlers.
  - `jsonrpc.rs` — JSON-RPC 2.0 framing.
  - `methods.rs` — method dispatch (`initialize`, `tools/list`, `tools/call`, `resources/list`, `resources/read`, `resources/templates/list`, `prompts/list`).
  - `tools.rs` — tool implementations (delegating to existing handlers).
  - `resources.rs` — resource URI handlers.
- Endpoint `POST /mcp` (JSON-RPC request → JSON-RPC response).
- Endpoint `GET /mcp` (SSE channel for server-initiated notifications).
- `Mcp-Session-Id` header for stateful sessions across multiple POSTs.
- Read-only tool catalogue (per `docs/04_api_mcp_spec.html` §5):
  - `whats_in_my_cc.search_sessions`
  - `whats_in_my_cc.get_session_graph`
  - `whats_in_my_cc.search_findings`
  - `whats_in_my_cc.explain_node`
  - `whats_in_my_cc.get_file_lineage`
  - `whats_in_my_cc.get_otel_trace`
- Resource URIs:
  - `whats-in-my-cc://sessions`
  - `whats-in-my-cc://sessions/{session_id}`
  - `whats-in-my-cc://sessions/{session_id}/graph`
  - `whats-in-my-cc://sessions/{session_id}/findings`
  - `whats-in-my-cc://findings/{finding_id}`
  - `whats-in-my-cc://file-lineage/{session_id}?file_path=…`
  - `whats-in-my-cc://otel/traces/{trace_id}`
- Same Origin / Host allowlist middleware as Pull API.
- Streaming notifications: when a server-side rebuild completes, push a `notifications/resources/updated` message to all connected SSE clients whose subscribed resource is affected.

### Out of scope

- WebSocket transport (MCP spec also lists; not needed for MVP).
- Write tools.
- Prompts (returns empty `prompts/list`).
- Sampling / completions.
- Stateful subscription resumption across server restarts.
- Auth (slice-19).

---

## 3. Protocol surface

MCP version pin: `2024-11-05`. The version is hard-coded; bumping requires a new slice that updates the compat fixtures.

### `initialize`

Request:

```json
{
  "jsonrpc": "2.0", "id": 1, "method": "initialize",
  "params": {
    "protocolVersion": "2024-11-05",
    "capabilities": { "roots": {}, "sampling": {} },
    "clientInfo": { "name": "claude", "version": "..." }
  }
}
```

Response:

```json
{
  "jsonrpc": "2.0", "id": 1,
  "result": {
    "protocolVersion": "2024-11-05",
    "capabilities": {
      "tools": { "listChanged": false },
      "resources": { "subscribe": true, "listChanged": false },
      "prompts": { "listChanged": false },
      "logging": {}
    },
    "serverInfo": { "name": "whats-in-my-cc", "version": "<crate version>" }
  }
}
```

`Mcp-Session-Id` header is set in the response and required on subsequent requests in the same logical session. Generated as `format!("mcps_{}", ulid::Ulid::new())`.

### `notifications/initialized`

Client → server, no response. Server marks the session as active.

### `tools/list`

```json
{
  "jsonrpc": "2.0", "id": 2,
  "result": {
    "tools": [
      {
        "name": "whats_in_my_cc.get_session_graph",
        "description": "Return the graph for a session.",
        "inputSchema": {
          "type": "object",
          "properties": {
            "session_id": { "type": "string" },
            "include_sources": { "type": "boolean", "default": true }
          },
          "required": ["session_id"]
        }
      },
      ...
    ]
  }
}
```

### `tools/call`

```json
{
  "jsonrpc": "2.0", "id": 3, "method": "tools/call",
  "params": {
    "name": "whats_in_my_cc.get_session_graph",
    "arguments": { "session_id": "aac68973" }
  }
}
```

Response wraps the existing Pull API's data shape inside `content` (per MCP convention):

```json
{
  "jsonrpc": "2.0", "id": 3,
  "result": {
    "content": [
      {
        "type": "text",
        "text": "{ ... data envelope from /v1/sessions/aac68973/graph ... }"
      }
    ],
    "isError": false
  }
}
```

JSON content is serialised into a `text` block (MCP convention). Future improvement: emit `resource` content blocks instead, but slice-17 starts simple.

### `resources/list`

Returns a paginated list of session-level resources. Templates (`resources/templates/list`) returns the URI templates with placeholders.

### `resources/read`

```json
{
  "jsonrpc": "2.0", "id": 4, "method": "resources/read",
  "params": { "uri": "whats-in-my-cc://sessions/aac68973" }
}
```

Response:

```json
{
  "jsonrpc": "2.0", "id": 4,
  "result": {
    "contents": [
      { "uri": "whats-in-my-cc://sessions/aac68973", "mimeType": "application/json", "text": "{ ... }" }
    ]
  }
}
```

### `notifications/resources/updated`

Server-initiated. Pushed over the SSE channel when rebuild_session completes:

```json
{
  "jsonrpc": "2.0", "method": "notifications/resources/updated",
  "params": { "uri": "whats-in-my-cc://sessions/aac68973/graph" }
}
```

---

## 4. Transport details

### POST `/mcp`

- Accepts `application/json`.
- Reads one or many JSON-RPC requests (per MCP, batch is allowed).
- Returns either a single response or a batch.
- Origin / Host header validated by existing middleware.
- `Mcp-Session-Id`: required after `initialize`; absent on `initialize`. Server creates one on `initialize` response.

### GET `/mcp`

- Accepts `text/event-stream`.
- Server holds the connection open and emits SSE events for notifications.
- The first event after connect is a synthetic `notifications/initialized` echo (per spec) so the client knows the channel is alive.
- Sessions are looked up via `Mcp-Session-Id`. Unknown session id ⇒ 404.

### Connection lifecycle

```
client ──POST initialize──> server
       <──response w/ Mcp-Session-Id
client ──POST notifications/initialized──> server
       (server marks session active)

client ──GET /mcp w/ Mcp-Session-Id──> server (SSE)
       <──event: notifications/initialized
       ...
       <──event: notifications/resources/updated   (when rebuild completes)
```

The SSE channel and POST channel are independent but bound by the same `Mcp-Session-Id`.

### Concurrency

A single SSE channel can be open per session id. Re-connecting closes the old one. The fan-out is implemented over a `tokio::sync::broadcast` channel held by the server state.

---

## 5. Origin + Host validation

Existing middleware (slice-1 DEV-06) already validates the `Host` header against an allowlist for Pull API. The MCP endpoint reuses it. Additionally, the MCP endpoint enforces the `Origin` header check that the security spec calls out (`docs/05_security_governance_spec.html` §3):

- If `Origin` is present and not in the allowlist (`http://127.0.0.1:*`, `http://localhost:*`), respond `403`.
- If `Origin` is absent (curl-style call from another CLI), allow.

Test: `tests/mcp_origin_validation.rs` covers both branches.

---

## 6. Schema lock — JSON-RPC + tool inputs

Each tool's input schema lives in `src/api/mcp/tools/<tool>.rs` as a `serde_json::Value` const-fn. The schemas are emitted in `tools/list` and asserted by a fixture-based test `tests/mcp_tools_list.rs` that loads the expected list from `tests/fixtures/mcp/tools_list_expected.json` and diff-asserts.

The fixture is the regression lock. Changing a tool's input schema requires updating the fixture in the same commit, which surfaces the change to PR review.

---

## 7. Resource registry

```rust
pub fn resource_templates() -> Vec<ResourceTemplate> {
    vec![
        ResourceTemplate {
            uri_template: "whats-in-my-cc://sessions/{session_id}".into(),
            name: "Session summary".into(),
            mime_type: Some("application/json".into()),
        },
        ResourceTemplate {
            uri_template: "whats-in-my-cc://sessions/{session_id}/graph".into(),
            name: "Session graph".into(),
            mime_type: Some("application/json".into()),
        },
        ResourceTemplate {
            uri_template: "whats-in-my-cc://sessions/{session_id}/findings".into(),
            name: "Session findings".into(),
            mime_type: Some("application/json".into()),
        },
        ResourceTemplate {
            uri_template: "whats-in-my-cc://findings/{finding_id}".into(),
            name: "Finding detail".into(),
            mime_type: Some("application/json".into()),
        },
        ResourceTemplate {
            uri_template: "whats-in-my-cc://file-lineage/{session_id}".into(),
            name: "File lineage".into(),
            mime_type: Some("application/json".into()),
        },
        ResourceTemplate {
            uri_template: "whats-in-my-cc://otel/traces/{trace_id}".into(),
            name: "OTel trace".into(),
            mime_type: Some("application/json".into()),
        },
    ]
}
```

`resources/list` enumerates concrete URIs by querying the DB for current sessions and findings. Pagination over MCP's `cursor` token mirrors Pull API.

---

## 8. Compat fixture

`tests/fixtures/mcp/protocol_compat.json` — frozen golden of:
- `initialize` request + expected response shape (with placeholders for the dynamic version + session_id).
- `tools/list` response.
- `resources/templates/list` response.

The test `tests/mcp_spec_compat.rs` asserts the actual responses match (modulo placeholders).

---

## 9. Failure modes

| Failure | Behaviour |
|---|---|
| Unknown `Mcp-Session-Id` on POST | `400` with JSON-RPC error `-32602` (Invalid params), `data.reason = "unknown_session"`. |
| Unknown method | JSON-RPC error `-32601` (Method not found). |
| Unknown tool name | JSON-RPC error in `tools/call` response with `isError: true`. |
| Unknown resource URI | `resources/read` response with `isError: true`. |
| SSE client disconnects | Server drops the broadcast receiver; rebuild notifications stop being emitted for that session. |
| Server restart | All `Mcp-Session-Id`s become invalid; clients must re-initialize. Logged in server stderr at boot. |

---

## 10. Deviations index (slice-17)

| ID | Description |
|---|---|
| DEV-S17-01 | We hand-implement MCP transport rather than depend on a Rust SDK crate. Reason: the surface we need is narrow + stable; adding a dep is disproportionate. The compat fixture protects against drift. |
| DEV-S17-02 | We pin to `protocolVersion: "2024-11-05"`. Bumping requires a new slice. |
| DEV-S17-03 | Tool outputs are serialised as a single `text` content block holding the JSON envelope as a string. Future could use MCP `resource` content type but slice-17 keeps it simple. |
| DEV-S17-04 | `Mcp-Session-Id` is in-memory only; server restart invalidates all sessions. Persistent sessions are post-MVP. |
| DEV-S17-05 | A single SSE channel per session id is allowed; reconnect closes the previous one. Multi-client subscription per session is post-MVP. |
| DEV-S17-06 | `prompts/list` returns an empty list. We do not currently ship MCP prompts. |
| DEV-S17-07 | Auth is **out of scope** in slice-17. Slice-19 adds bearer-token enforcement to both `/v1/*` and `/mcp`. |

---

## 11. Commit plan summary

See `2026-05-27-witmcc-slice17-mcp-streamable-http.md`. Seven commits:

1. `test(slice-17): red-locking tests for MCP transport, methods, tools, resources, compat`
2. `feat(api/mcp): JSON-RPC framing + transport skeleton (POST + GET-SSE)`
3. `feat(api/mcp): initialize + notifications/initialized + Mcp-Session-Id lifecycle`
4. `feat(api/mcp): tools/list + tools/call dispatcher + 6 tool impls`
5. `feat(api/mcp): resources/list + resources/templates/list + resources/read + URI parser`
6. `feat(api/mcp): rebuild_session → notifications/resources/updated fan-out`
7. `feat(api/mcp): Origin validation + compat golden test`
