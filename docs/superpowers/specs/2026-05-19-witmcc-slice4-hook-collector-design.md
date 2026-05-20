# Slice-4 Design — Hook Collector (External Claude Code Hooks)

**Date:** 2026-05-19
**Branch:** `slice4-hook-collector` (based on `main` post slice-3)
**Goal:** Receive Claude Code hook events directly over HTTP and persist them as `RawEvent` + `ObservedEvent`, so that hook-lifecycle observations join transcript and OTel as first-class ingest sources on the same session timeline.

---

## 1. Motivation

PRD OBS-3 mandates collecting "session, turn, tool, permission, subagent, compaction, MCP elicitation, worktree" hook lifecycle events. Slice-1 only consumes hook *attachments embedded inside* transcript JSONL records (`hook_success` / `hook_additional_context`), which is a partial, post-hoc view; the live hook lifecycle (e.g. `PreToolUse`, `Notification`, `PreCompact`) never gets captured.

MVP M1 (Source ingestion) acceptance requires a hook receiver alongside transcript and OTel. Without it:

- **AC-1** (single-session replay) is partial: the timeline misses live hook beats — for example, `PreToolUse` precedes the tool call but only `PostToolUse` reaches the transcript.
- **OBS-3 degrade semantics** (hook failure must not break Claude Code itself) is untested.
- Permission / risky-action findings in M5 depend on permission-hook events that the transcript path never sees.

This slice **proves the live hook path end-to-end**: user configures a Claude Code hook command that POSTs JSON to us; we ingest, store, surface in the existing graph and UI lanes.

---

## 2. Scope

### In Scope

- **`POST /hooks/v1/events`** receiver accepting Claude Code hook JSON (single object **or** array, both shapes).
- Pass-through ingest: the raw Claude Code hook payload is stored verbatim in `raw_event.payload`; only known fields are extracted for `observed_event`.
- New `RawEvent.source_type` value `"hook"` (in addition to existing `claude_transcript`, `otel`).
- Reuse `EventKind::HookEvent` (already present from slice-1). The `subkind` carries the hook event name (`pre_tool_use`, `post_tool_use`, etc.).
- Idempotent dedup via `payload_sha256` (same canonical body → no-op), reusing `repo_raw::insert_dedup`.
- Per-touched-session graph rebuild after ingest (slice-3 DEV-S3-07 policy).
- Graph node materialisation for external hook events (reuse `hook_event` `node_kind`; merge_keys include `session_id` + `hook_event_name` + `tool_use_id` when present).
- UI: ensure a **Hook lane** exists on Timeline; add a SourcePanel section for hook records showing extracted attributes (event name, tool, decision, etc.).
- Hand-crafted JSON fixtures covering each Claude Code hook event name.
- Integration tests posting through the live HTTP server (axum-test pattern from slice-3).
- README section documenting the user-side hook command to register.
- Schema bump 0.2.0 → 0.3.0.

### Out of Scope (deferred)

- File watcher / live transcript tail (slice-5 file-git scope adjacent; transcript tail itself a separate candidate).
- Hook ↔ transcript deduplication (when transcript replays the same hook as an attachment, two rows survive; a future slice may merge by correlation keys).
- Redaction of `tool_input` / `prompt` payloads (M7).
- Token authentication, Origin enforcement beyond existing host allowlist (M7).
- New CLI subcommand (the existing `serve` already binds the receiver router).
- Findings engine consumption of hook signals (M5).
- MCP server exposure (M6).

---

## 3. Architecture

```
┌────────────────────────┐                                  ┌──────────────────────┐
│ Claude Code (user)     │  stdin JSON → hook command       │ /hooks/v1/events     │
│   hooks/PreToolUse,    │  ──────────────────────────────▶ │ axum POST handler    │
│   PostToolUse, ...     │  curl --data-binary @- ...       │  - body ≤ 1 MB       │
│   (any of 9 events)    │                                  │  - JSON parse        │
└────────────────────────┘                                  │  - single or batch   │
                                                            └──────────┬───────────┘
                                                                       │ Vec<HookRecord>
                                                                       ▼
                                                            ┌──────────────────────┐
                                                            │ src/ingest/hook.rs   │
                                                            │  parse Claude Code   │
                                                            │  hook JSON shape     │
                                                            │  → RawEvent          │
                                                            │  → ObservedEvent     │
                                                            │  (idempotent dedup   │
                                                            │   by payload_sha256) │
                                                            └──────────┬───────────┘
                                                                       │
                                                                       ▼
                                                            ┌──────────────────────┐
                                                            │ SQLite               │
                                                            │   raw_event          │
                                                            │   observed_event     │
                                                            │   graph rebuilt      │
                                                            │   per session        │
                                                            └──────────────────────┘
```

Storage and graph rebuild reuse `repo_raw`, `repo_observed`, `repo_runs`, and `graph::build::rebuild_session` — same primitives slice-3 already exercises.

---

## 4. API Surface

### `POST /hooks/v1/events`

**Request**

| Header             | Required | Notes |
|--------------------|----------|-------|
| `Content-Type`     | yes      | `application/json` |
| `Content-Encoding` | optional | `gzip` accepted (reusing OTel decode helper); no other encodings |
| body               | yes      | Single Claude Code hook JSON object **or** JSON array of such objects (≤ 1 MB after decompression) |

**Single-event body shape** (pass-through from Claude Code's stdin format):

```jsonc
{
  "session_id":      "sess_abc",
  "transcript_path": "~/.claude/projects/.../sess_abc.jsonl",   // optional
  "cwd":             "/Users/me/proj",                          // optional
  "hook_event_name": "PreToolUse",                              // required, one of 9 known names
  "tool_name":       "Bash",                                    // present for tool hooks
  "tool_input":      { "command": "ls" },                       // tool hooks only
  "tool_response":   { ... },                                   // PostToolUse only
  "tool_use_id":     "toolu_01...",                             // tool hooks (correlation key)
  "prompt":          "...",                                     // UserPromptSubmit only
  "message":         "...",                                     // Notification only
  "trigger":         "auto" | "manual",                         // PreCompact only
  "source":          "startup" | "resume" | "clear",            // SessionStart only
  "reason":          "...",                                     // Stop / SubagentStop, optional
  "timestamp":       "2026-05-19T09:12:00+09:00"                // optional, defaults to receive time
}
```

**Recognised `hook_event_name` values** (Claude Code v1.x catalogue):

| Name | Category (PRD OBS-3) | `subkind` (snake_case) | Required Claude Code fields beyond base |
|------|---------------------|-----------------------|-----------------------------------------|
| `PreToolUse`         | tool         | `pre_tool_use`         | `tool_name`, `tool_input`, `tool_use_id` |
| `PostToolUse`        | tool         | `post_tool_use`        | `tool_name`, `tool_input`, `tool_response`, `tool_use_id` |
| `UserPromptSubmit`   | turn         | `user_prompt_submit`   | `prompt` |
| `Stop`               | turn         | `stop`                 | — |
| `SubagentStop`       | subagent     | `subagent_stop`        | — |
| `Notification`       | permission   | `notification`         | `message` |
| `PreCompact`         | compaction   | `pre_compact`          | `trigger` |
| `SessionStart`       | session      | `session_start`        | `source` |
| `SessionEnd`         | session      | `session_end`          | — |

Unknown `hook_event_name` values are stored with `subkind = "unknown"` and `event_kind = HookEvent` (forward-compat: future Claude Code hooks ingest without code change).

**Batch body shape** — same fields, but top-level is a JSON array:

```jsonc
[ {hook1}, {hook2}, ... ]
```

A single request accepts either shape; we detect by `is_array()`.

**Response (200 OK)**

```json
{
  "meta": {"schema_version": "0.3.0", "generated_at": "<rfc3339>"},
  "data": {
    "accepted_events":  3,
    "rejected_events":  0,
    "duplicate_events": 1,
    "sessions_touched": ["sess_abc"]
  }
}
```

**Error codes**

- `400 Bad Request` — body is not valid JSON, or top-level is neither object nor array.
- `413 Payload Too Large` — body exceeds 1 MB before decompression.
- `415 Unsupported Media Type` — non-JSON content type or unsupported encoding.
- `500` — DB write failure.

Per-event errors (missing `hook_event_name`, missing `session_id`) **do not fail the request**; the offending event is counted in `rejected_events` and other events in the batch still ingest.

### Existing endpoints (unchanged shape, expanded data)

- `GET /v1/sessions` — sessions that received external hooks appear.
- `GET /v1/sessions/:id` — `events[]` now may include records with `kind="hook_event"` originating from `source_type="hook"`.
- `GET /v1/sessions/:id/graph` — `hook_event` nodes now may include external-hook-only entries (no transcript twin).
- `GET /v1/events/:event_id/raw` — returns the original Claude Code hook JSON under `record`.

---

## 5. Data Model Changes

### 5.1 `RawEvent.source_type` enum (effective)

`raw_event.source_type` remains TEXT (no DB constraint). We add `"hook"` to the documented enum: `claude_transcript`, `otel`, `hook`. Spec section 03 of `docs/03_data_model_spec.html` already lists `hook` in the allowed values.

### 5.2 `ObservedEvent` fields used for hook records

Existing struct fields are sufficient (no new column):

| Field | Hook event value |
|-------|------------------|
| `actor`         | `Hook` (existing variant) |
| `kind`          | `HookEvent` (existing variant) |
| `subkind`       | snake_case hook event name (see §4 table) |
| `tool_name`     | from `tool_name` if present |
| `tool_use_id`   | from `tool_use_id` if present |
| `session_id`    | from `session_id` (required) |
| `cwd`           | from `cwd` if present |
| `observed_at`   | from `timestamp` if present, else server receive time |
| `payload`       | `{ "hook": <verbatim Claude Code JSON> }` |
| `parser_version`| `hook_parser.v1` (new constant) |

`telemetry`, `trace_id`, `span_id` etc. stay `None` for hook events. A future slice may correlate hooks to OTel spans via `tool_use_id` + time-window heuristic.

### 5.3 `EventKind`

No new variant. Reuse `EventKind::HookEvent` (already serialises as `"hook_event"`).

### 5.4 Schema version

Bump `SCHEMA_VERSION` from `0.2.0` to `0.3.0` (new `source_type` value in active use; `parser_version_hook` constant introduced).

### 5.5 `PARSER_VERSION_HOOK` constant

Add to `src/model/meta.rs` alongside existing `PARSER_VERSION_OTEL`:

```rust
pub const PARSER_VERSION_HOOK: &str = "hook_parser.v1";
```

### 5.6 Migration

**None.** No new column, no new index. Existing `observed_event` and `raw_event` accommodate hook records as-is. (A later slice may add an index on `(source_type, session_id)` if hook-only sessions become large; deferred.)

### 5.7 Idempotency / `event_id` derivation

- `raw_event` dedup: `(source_uri, source_line_no, payload_sha256)` UNIQUE constraint from slice-1 still applies. We synthesize a deterministic `source_uri = "hook://<session_id>/<hook_event_name>/<tool_use_id-or-empty>"` (analogous to slice-3 OTel `source_uri`). Combined with `payload_sha256` of canonical hook JSON, re-POSTing the same event is a no-op.
- `event_id` for the matching `observed_event` is a fresh ULID; idempotency is guaranteed by the raw dedup short-circuiting the observed insert (see store flow in §6).

### 5.8 RawEvent for hooks

| Column              | Value |
|---------------------|-------|
| `source_type`       | `"hook"` |
| `source_uri`        | `hook://<session_id>/<hook_event_name>/<tool_use_id or "">` |
| `source_line_no`    | `0` (single-event POST has no positional meaning) |
| `source_byte_offset`| `0` |
| `payload_sha256`    | sha256 of canonical hook JSON (recursive key sort, same helper as slice-3) |
| `payload`           | canonical hook JSON bytes |
| `parse_error`       | `null` on success; error string when event was rejected but still persisted (we persist rejected raw rows with the reason so the user can debug) |

---

## 6. Ingest Flow (`src/ingest/hook.rs`, new module)

```rust
pub struct HookRecord {
    pub session_id:      String,
    pub hook_event_name: String,
    pub subkind:         String,
    pub tool_name:       Option<String>,
    pub tool_use_id:     Option<String>,
    pub cwd:             Option<String>,
    pub timestamp:       Option<DateTime<Utc>>,
    pub raw:             Value,
}

pub struct RejectedHook { pub reason: String, pub raw: Value }

pub struct ParseResult {
    pub events:   Vec<HookRecord>,
    pub rejected: Vec<RejectedHook>,
}

pub fn parse_body(body: &Value) -> ParseResult { ... }   // accepts object OR array
pub async fn store(pool: &SqlitePool, parsed: ParseResult, received_at: DateTime<Utc>) -> Result<IngestResult>
```

`store` mirrors `src/ingest/otel.rs::store` exactly, including the **self-heal pattern from DEV-S3-07** (`touched.insert` happens **before** the dedup check, so re-POST after a binary upgrade still triggers graph rebuild).

`parse_body`:

1. If `body.is_array()`: iterate, parse each element.
2. Else if `body.is_object()`: parse single element.
3. Else: returns `ParseResult { rejected: [{reason: "body must be object or array", raw: body.clone()}] }`.

`parse_one`:

- Requires `hook_event_name` (string) and `session_id` (non-empty string). Otherwise reject.
- Derives `subkind` from `hook_event_name` via the §4 table; unknown name → `subkind = "unknown"` (still accepted).
- Extracts the optional fields (`tool_name`, `tool_use_id`, `cwd`, `timestamp`).
- `raw` is the verbatim hook JSON.

---

## 7. Graph Mapping (`src/graph/build.rs`)

Slice-1 already materialises `hook_event` graph nodes from transcript-internal hook attachments. The `compute` function's `EventKind::HookEvent` branch already exists; we extend it so:

- When `source_type = "hook"` (external) or the existing internal case: `node_kind = "hook_event"`.
- `merge_keys` JSON:
  ```jsonc
  {
    "session_id":      "<session>",
    "hook_event_name": "<original casing>",
    "tool_use_id":     "<id or null>"
  }
  ```
- External and internal hook events with the **same** `session_id` + `hook_event_name` + `tool_use_id` deduplicate onto the same node (existing `node_index_by_id` dedup handles it once merge_keys are equal). This means a future where transcript producers don't double-record gets free dedup; today, since transcript hook events do not currently carry `tool_use_id` in the same form, dedup will be partial — accepted as a known gap (documented in implementation-notes as DEV-S4-XX).

No new edge kind in slice-4. (`tool_call_to_hook` correlation is a follow-up.)

---

## 8. UI Changes (`webui/`)

### 8.1 Lane mapping

Ensure `webui/src/api/laneMapping.ts` maps `'hook_event'` to a `Hook` lane. If the slice-2 six-lane mapping already includes Hook, no change; otherwise add the entry. The Timeline component already iterates all declared lanes.

### 8.2 SourcePanel

`webui/src/components/SourcePanel.tsx` currently shows raw `record` JSON; OTel records also get an Attributes summary (slice-3). Extend the panel:

- When `record_type === 'hook_event'`:
  1. Header strip: `hook_event_name`, optional `tool_name`, optional `tool_use_id`.
  2. Body-specific summary depending on subkind:
     - `pre_tool_use` / `post_tool_use`: show `tool_input` (and `tool_response` if PostToolUse) as collapsible JSON sections.
     - `user_prompt_submit`: show `prompt` as text.
     - `notification`: show `message`.
     - `pre_compact`: show `trigger`.
     - `session_start`: show `source`.
     - others: just the header strip.
  3. Below: full raw hook JSON via the existing `JsonView`.

### 8.3 Timeline

No structural change. The Hook lane (whether new or existing) carries hook markers. Visual differentiator (e.g. small icon or color) is optional — out of scope for slice-4.

### 8.4 SessionDetailPage

No layout change. The `by_kind` summary already aggregates `hook_event` count; external hooks will inflate that count naturally.

---

## 9. Error Handling & Edge Cases

| Case | Behaviour |
|------|-----------|
| Body is `null` / number / string | `400 Bad Request` (top-level must be object or array). |
| Object missing `hook_event_name` | Event rejected, counted in `rejected_events`; other batch items unaffected. |
| Object missing `session_id` or empty | Event rejected. (No anonymous hooks — they wouldn't surface in `/v1/sessions` anyway.) |
| Unknown `hook_event_name` (e.g. future Claude Code event) | Accepted with `subkind = "unknown"`. Surface in UI as raw JSON. |
| Same body POSTed twice | First time: ingested. Second time: `duplicate_events += 1`, no new row. Session still marked touched → graph rebuild runs (self-heal). |
| `tool_use_id` collision with existing transcript-internal hook attachment | Two rows in `observed_event` (different `event_id`, different `source_type`); graph dedup may collapse if merge_keys match exactly. Documented as known gap. |
| Hook event timestamp older than session start | Stored as-is; surfaces in timeline at its own `observed_at`. (Replay UI sorts by `observed_at`.) |
| Body decompresses past 1 MB | `400 Bad Request`. |
| Non-UTF-8 / invalid JSON | `400 Bad Request`. |
| Concurrent POSTs to the same session | Each POST runs its own `repo_runs::start`/`finish` and rebuild — last writer wins on graph view. SQLite WAL handles row-level concurrency. |

---

## 10. Test Strategy

### 10.1 Fixtures

`tests/fixtures/hook/` (new):

- `pre_tool_use.json` — single PreToolUse for Bash with `tool_use_id`.
- `post_tool_use.json` — matching PostToolUse with `tool_response`.
- `user_prompt_submit.json` — UserPromptSubmit with prompt text.
- `notification.json` — Notification with `message`.
- `pre_compact.json` — PreCompact with `trigger=auto`.
- `session_start.json` — SessionStart with `source=startup`.
- `batch_three.json` — JSON array of three different hook events for the same session.
- `missing_session_id.json` — invalid: PreToolUse with no `session_id`.
- `unknown_event.json` — `hook_event_name: "FutureHook"`.

### 10.2 Unit tests (in `src/ingest/hook.rs`)

- Single-object body parses into one `HookRecord`.
- Array body of N elements parses into N records.
- Missing `hook_event_name` → rejected.
- Missing `session_id` → rejected.
- Unknown `hook_event_name` → accepted with `subkind = "unknown"`.
- All nine known hook names map to the §4 subkind table.
- Canonical JSON sort yields byte-stable output for re-POST dedup.

### 10.3 Integration tests (`tests/hook_ingest.rs`, new)

- POST `pre_tool_use.json` → 200, `accepted_events=1`. Verify `/v1/sessions/<sid>` includes the event with `kind=hook_event`, `subkind=pre_tool_use`.
- POST same body twice → second call returns `duplicate_events=1`, `accepted_events=0`.
- POST `batch_three.json` → 200, `accepted_events=3`; `/v1/sessions/<sid>/graph` returns three `hook_event` nodes.
- POST `missing_session_id.json` → 200, `accepted_events=0, rejected_events=1`.
- POST `unknown_event.json` → 200, accepted; observed_event row carries `subkind="unknown"`.
- gzip-encoded POST decodes correctly (reuse slice-3 gzip path if shared; otherwise dedicated test).
- POST to `/hooks/v1/events` from `axum-test` client without `Host` header still succeeds (host_allowlist fallback from DEV-06).

### 10.4 UI tests (`webui/`)

- `laneMapping.test.ts`: `laneForNodeKind('hook_event') === 'Hook'`.
- `SourcePanel.test.tsx`: rendering a `hook_event` record with subkind `pre_tool_use` shows header + `tool_input` JSON section + raw JSON.
- `SourcePanel.test.tsx`: rendering a `notification` subkind shows `message` text.
- Regression: existing OTel-on-OTel-lane test still passes.

### 10.5 Acceptance smoke

```bash
target/debug/witmcc serve --bind 127.0.0.1 --port 7878 &
curl -X POST http://127.0.0.1:7878/hooks/v1/events \
  -H 'content-type: application/json' \
  --data-binary @tests/fixtures/hook/pre_tool_use.json
curl http://127.0.0.1:7878/v1/sessions/sess_test_A \
  | jq '.events[] | select(.kind=="hook_event")'
```

Open `http://127.0.0.1:7878/` → session list → click the session → timeline shows a marker on Hook lane → click → SourcePanel renders extracted summary + raw JSON.

---

## 11. Routing & Wiring

`src/api/routes.rs` (or equivalent) adds:

```rust
let router = router
    .route("/hooks/v1/events", post(hook::ingest_events));
```

`src/api/hook.rs` (new) mirrors `src/api/otel.rs`:

```rust
pub async fn ingest_events(
    State(pool): State<SqlitePool>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Envelope<HookIngestResponse>>, (StatusCode, Json<Value>)> {
    let json = decode_body(&headers, body, MAX_HOOK_BODY)?;
    let parsed = ingest::hook::parse_body(&json);
    let result = ingest::hook::store(&pool, parsed, Utc::now()).await?;
    Ok(Json(Envelope::wrap(result)))
}
```

`decode_body` should be a shared helper (extract from `src/api/otel.rs` if not already, or duplicate locally and refactor later). Limit constant `MAX_HOOK_BODY = 1 * 1024 * 1024`.

`host_allowlist` and `loopback` middleware apply unchanged. No new auth surface.

---

## 12. User-Side Hook Registration

README (and implementation-notes) document the user-facing wiring. Example `~/.claude/settings.json` snippet:

```jsonc
{
  "hooks": {
    "PreToolUse":  [{ "matcher": "*", "hooks": [{ "type": "command", "command": "/usr/local/bin/witmcc-forward.sh" }] }],
    "PostToolUse": [{ "matcher": "*", "hooks": [{ "type": "command", "command": "/usr/local/bin/witmcc-forward.sh" }] }],
    "UserPromptSubmit": [{ "hooks": [{ "type": "command", "command": "/usr/local/bin/witmcc-forward.sh" }] }],
    "Notification": [{ "hooks": [{ "type": "command", "command": "/usr/local/bin/witmcc-forward.sh" }] }],
    "PreCompact":  [{ "hooks": [{ "type": "command", "command": "/usr/local/bin/witmcc-forward.sh" }] }],
    "SessionStart":[{ "hooks": [{ "type": "command", "command": "/usr/local/bin/witmcc-forward.sh" }] }],
    "SessionEnd":  [{ "hooks": [{ "type": "command", "command": "/usr/local/bin/witmcc-forward.sh" }] }],
    "Stop":        [{ "hooks": [{ "type": "command", "command": "/usr/local/bin/witmcc-forward.sh" }] }],
    "SubagentStop":[{ "hooks": [{ "type": "command", "command": "/usr/local/bin/witmcc-forward.sh" }] }]
  }
}
```

`witmcc-forward.sh`:

```bash
#!/bin/bash
exec curl -sS -m 2 -X POST \
  -H 'content-type: application/json' \
  --data-binary @- \
  http://127.0.0.1:7878/hooks/v1/events > /dev/null 2>&1 || true
```

`-m 2` (2 s timeout) and `|| true` together implement **OBS-3 degrade semantics**: hook collector outage or hang never breaks Claude Code execution; the curl exits silently.

The forward script is **not** installed by witmcc; the user copies it. Auto-install is intentionally out of scope (CLAUDE.md non-goal: "Claude Code 설정 / hook / command / skill / memory 변경").

---

## 13. Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| Hook collector crashes/hangs and breaks Claude Code | User-side forward script uses curl with `-m 2 || true`. We document this in README. We never accept connections that could block: handler is async and returns within DB write latency. |
| Hook payload contains secrets (e.g. `tool_input.password`) | Redaction is M7. Document as known gap in implementation-notes. Recommend using `local_full_evidence` profile only in trusted contexts. |
| User accidentally posts batch >1 MB | `413 Payload Too Large`. Document the limit in README. |
| Schema drift in Claude Code's hook JSON (new fields, renames) | Source-preserving: raw payload kept in `raw_event.payload`. Parser extracts only known fields; unknown additions are persisted but not indexed. |
| Same hook posted from multiple shells concurrently | SQLite WAL + `repo_raw::insert_dedup` UNIQUE constraint serialises. Second call is no-op + `duplicate_events += 1`. |
| External hook + transcript hook attachment double-counts in `by_kind` | Documented as known gap. Future slice can dedupe via `tool_use_id` correlation. |

---

## 14. Migration Path

1. `cargo install` consumers run new `witmcc serve` — schema bump 0.2.0 → 0.3.0; no DDL change.
2. Existing rows from slices 1–3 continue working; `source_type` enum just gains a new accepted value.
3. UI is forward-compatible; the new SourcePanel branch only triggers when `record_type === 'hook_event'` and the existing rendering for transcript-internal hook events still works.
4. Users opt into live hook capture by adding the forward script + settings.json entries. Until they do, nothing changes for them.

---

## 15. Build / Dev Workflow

No new build steps. The receiver is in the existing binary; UI build remains `just webui-build && cargo build`.

Manual smoke (from §10.5 above) is the canonical dev loop.

---

## 16. Acceptance Criteria for Slice-4

1. `POST /hooks/v1/events` accepts a single PreToolUse JSON object and returns `200` with `accepted_events=1`.
2. `POST /hooks/v1/events` accepts an array of three hook events and returns `200` with `accepted_events=3`.
3. The session referenced by the hook(s) appears in `GET /v1/sessions` after ingest (if not present before).
4. `GET /v1/sessions/:id` includes records with `kind="hook_event"` and the correct `subkind` for each of the nine known Claude Code hook event names.
5. `GET /v1/sessions/:id/graph` contains `hook_event` nodes with `merge_keys.hook_event_name` set; nodes representing matching external + transcript-internal events with identical correlation keys deduplicate.
6. Re-POSTing the same body increments `duplicate_events` and does not create new rows; `sessions_touched` still includes the session (self-heal pattern from DEV-S3-07 verified).
7. Rejected events (missing `session_id` or `hook_event_name`) increment `rejected_events`; valid events in the same batch still ingest.
8. SourcePanel renders a hook record with extracted subkind summary + raw JSON; Timeline shows a marker on the Hook lane for each hook event.
9. README documents the forward-script pattern with degrade semantics (`-m 2 || true`).
10. All previously passing cargo tests + webui vitest tests still pass; new hook integration tests (≥6) and unit tests (≥6) added.

---

## 17. Open Decisions (resolved for this slice)

| Decision | Choice | Rationale |
|---|---|---|
| Channel | HTTP POST | Axum reuse, zero new deps, parallels slice-3 OTel pattern. User wires via a tiny curl forward script. |
| Wrapper vs pass-through | Pass-through | Claude Code's stdin JSON is already a clean schema; wrapping adds friction and breaks transparent dedup. |
| Single vs batch | Both | Users with multiple hooks can batch in their forward script for fewer requests; tests cover both. |
| EventKind | Reuse `HookEvent` | Source distinction via `source_type` (`hook` vs `claude_transcript`). Avoids EventKind explosion. |
| Schema bump | 0.3.0 | New `source_type` enum value in active use; new parser version constant. Existing 0.2.0 rows readable. |
| Dedup against transcript-internal hooks | Document as known gap | Transcript and external hook may both record `PreToolUse` for the same tool call; dedup requires correlation keys not always present today. Future slice. |
| Redaction | Not in slice-4 | M7. Hook payloads can carry secrets; warn in README. |
| Authentication | Loopback + host allowlist only | Same surface as slices 1–3. Token auth deferred to M7. |
| CLI changes | None | Existing `serve` binds the new route automatically. |
| New migration | None | No new column/index strictly required. Future index on `(source_type, session_id)` deferred. |

---

## 18. Follow-up slices unblocked by this work

- **Slice-5 (file-git)**: hook-derived tool calls become anchor nodes for `mutates` edges from file events.
- **Findings engine v1 (M5)**: `risky_action` finding depends on `Notification` permission hooks; `tool_failure` finding benefits from `PostToolUse` status.
- **Hook ↔ transcript dedup**: separate cleanup slice once correlation key coverage is complete.
- **OTel ↔ hook correlation**: hook `tool_use_id` + OTel span attribute `tool.use_id` can join, enabling cross-kind edges.
