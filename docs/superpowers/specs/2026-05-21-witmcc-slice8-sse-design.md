# Slice-8 Design — WebUI Live Updates via SSE (Streamable HTTP foundation)

**Date:** 2026-05-21
**Branch:** `slice8-sse-live` (based on `main` post slice-7)
**Goal:** Make every WebUI page update without a manual refresh. Land an in-process pub/sub channel from every ingest writer to an SSE endpoint that the WebUI (and, in M6, the MCP Streamable HTTP transport) subscribes to. The smoothness problem the user reported after slice-7 is the trigger; the same primitives become the M6 foundation, so this slice is also the first half of the Pull API streaming work.

---

## 1. Motivation

After slice-7 every source is ingested live, but the WebUI still re-fetches only on mount and on manual refresh. The data is fresh on the server, the gap is purely in the read path. Two facts make a stream-based fix the right move (not polling):

1. **`docs/06_mvp_execution_plan.html` already promises "streaming status events" under M6 (Pull API and MCP).** The MCP Streamable HTTP transport is itself an HTTP POST → SSE response. Building one in-process broadcast channel + one SSE endpoint here delivers the WebUI fix today and the MCP streaming surface in M6 without re-doing the wiring.
2. **Polling has no clean MCP analogue.** MCP clients expect a streaming resource. Polling-now-then-stream-later means writing throwaway client code and an awkward server endpoint that the MCP transport can't reuse.

The slice ends when (a) opening any WebUI page and leaving it alone shows new Claude Code activity arriving smoothly without a reload, and (b) the same channel is wired in a way that the M6 MCP server can subscribe directly.

---

## 2. Scope

### In Scope

- **In-process pub/sub** — one `tokio::sync::broadcast::Sender<LiveEvent>` lives in `AppState` and is passed to every ingest writer. Capacity `512`. One global channel, no sharding (premature for MVP scale).
- **`LiveEvent` envelope** — minimal `{schema_version, session_id, event_id, kind, source_type, observed_at}`. No full normalized payload; clients fetch detail via existing GET endpoints. Frozen as v1; future fields bump `schema_version`.
- **`LiveSink` trait** — `Broadcast(Arc<Sender<LiveEvent>>)`, `Noop` (for CLI `ingest --all`), `Capturing(Arc<Mutex<Vec<LiveEvent>>>)` (test helper). All `ingest::*` store/write functions take `&dyn LiveSink` (not `Option<...>`), so the type system enforces wiring instead of `None`-by-default silently disabling live emission.
- **SSE endpoint** — `GET /v1/stream` (all sessions) and `GET /v1/stream?session=<id>` (single session). Server-side filter applied to both backfill and forwarded broadcast. Both paths share one handler.
- **Last-Event-ID resume** — standard EventSource header (or explicit `?last_event_id=` for non-browser clients). Server subscribes-before-SELECTs to close the race window, deduplicates by `event_id` seen during backfill.
- **`event: gap`** — if `broadcast::Receiver` returns `Lagged(n)` the handler emits one SSE frame `event: gap\ndata: {"since": "<last_id>"}\n\n` and continues. Client recovers by issuing GET + reconnecting.
- **`event: resync`** — if `Last-Event-ID` is well-formed (ULID) but no row matches (DB reset, retention purge, future cursor), backfill is skipped and the first frame is `event: resync\ndata: {"reason": "unknown_cursor"}\n\n` followed by normal live forwarding. Client wipes local cache and re-fetches baseline.
- **Keepalive** — `:keepalive\n\n` SSE comment every 30s of idle. Configurable via `--sse-keepalive-secs <5..=120>`.
- **WebUI changes**
  - `SessionListPage` opens `EventSource('/v1/stream')`. On envelope: bump matching row's `event_count` + `last_observed_at`; if `session_id` is new, prepend a row. LIVE badge becomes envelope-based (see §6).
  - `SessionDetailPage` opens `EventSource('/v1/stream?session=' + id)`. On envelope: fetch `/v1/events/:id/raw` only if the user is viewing it; otherwise append a Timeline marker from envelope alone (kind + observed_at are sufficient to render a node).
  - Both: handle `event: gap` and `event: resync` by clearing local state and re-issuing the corresponding GET, then re-attaching to the live stream.
- **Test policy** — every existing test that calls `router(pool)` or `ingest_file(pool, path)` migrates in the same commit; no half-migrated state in any commit. Production wiring is guarded by L3 subprocess tests (one per ingest path, 7 total), not just signature checks.

### Out of Scope (deferred)

- **Per-session broadcast sharding.** Single channel is fine for ≤ 100 sessions × ≤ 100 events/min. Sharding becomes a separate slice when there's a measurement.
- **MCP server endpoint.** This slice provides the channel and the SSE wire format; M6 wraps it in MCP Streamable HTTP. We do not ship `/mcp/*` here.
- **Authentication / Origin enforcement on the stream.** Inherits the existing `host_allowlist` + loopback bind from slice-1. Token-based auth (M7) layers on top later.
- **Compression** (`Content-Encoding: gzip` on SSE). Premature for loopback.
- **Findings engine, Insight surface, Why panel.** M5 + M4 boundaries unchanged.
- **Backfill compression / pagination.** If the cursor is far behind, backfill streams as one frame per row. Slow client lagging out is detected by `event: gap` and resolved by the resync path.

---

## 3. Architecture

```
┌──────────────────── witmcc serve (single process) ─────────────────────┐
│                                                                        │
│  ingest writers (all gain &dyn LiveSink):                              │
│    · transcript_tail::run         ─┐                                   │
│    · watcher (file events)        ─┤                                   │
│    · git_poller                   ─┼─► sqlx COMMIT ─► sink.emit(env)   │
│    · api::otel POST handlers      ─┤                                   │
│    · api::hook POST handler       ─┘                                   │
│    · CLI ingest --all : LiveSink::Noop (no broadcast send)             │
│                                                                        │
│              AppState ── Arc<broadcast::Sender<LiveEvent>>             │
│                                  │                                     │
│                                  │ .subscribe()                        │
│                                  ▼                                     │
│   axum SSE handler  GET /v1/stream[?session=<id>]                      │
│     1. rx = sender.subscribe()                       ← FIRST           │
│     2. backfill = SELECT event_id,kind,session_id,source_type,         │
│                          observed_at FROM observed_event               │
│                   WHERE event_id > :last_event_id                      │
│                   [AND session_id = :session]                          │
│                   ORDER BY event_id ASC                                │
│        emit each as `data: {...}` frame; record seen_ids               │
│     3. loop {                                                          │
│          select! {                                                     │
│            ev = rx.recv() => match ev {                                │
│              Ok(env) if env.session_id_matches(filter)                 │
│                       && !seen_ids.contains(&env.event_id)             │
│                 => emit `id: ...\ndata: {...}\n\n`,                    │
│              Err(Lagged(_)) => emit `event: gap`,                      │
│              Err(Closed) => break,                                     │
│            }                                                           │
│            _ = keepalive.tick() => emit `:keepalive\n\n`,              │
│            _ = req.disconnected() => break,                            │
│          }                                                             │
│        }                                                               │
└────────────────────────────────────┼───────────────────────────────────┘
                                     │ text/event-stream HTTP/1.1
                                     ▼
                       Browser EventSource (WebUI)
                       MCP client (M6 — reuses same handler)
```

Key invariants:

1. **`subscribe()` runs before the SELECT.** Any INSERT committed between SELECT and subscribe would be lost; the `seen_ids` set absorbs duplicates created by INSERTs between subscribe and SELECT.
2. **`sink.emit(env)` is called only on `Ok(_)` from the sqlx tx commit**, so a failed INSERT never reaches subscribers.
3. **The handler holds no DB transaction across the loop.** Long-lived SQL handles would starve other writers; backfill closes its statement before entering the loop.
4. **One process, no IPC.** The `Sender` and every `Receiver` live in the same tokio runtime memory; this is a function call, not a message broker.

---

## 4. Schema — `LiveEvent` envelope (v1, frozen)

```jsonc
{
  "schema_version": "1",       // bump for additive changes
  "session_id":  "<uuid-or-empty-string>",
  "event_id":    "<ULID>",     // sortable, monotonic-ish per writer
  "kind":        "user_message" | "assistant_message" | "tool_call"
                | "tool_result" | "file_history_snapshot" | "hook_event"
                | "metric_sample" | "log_record" | "otel_span"
                | "file_event" | "git_commit" | "diff_hunk"
                | "system" | "unknown",
  "source_type": "transcript" | "otel" | "otel-metrics" | "otel-logs"
                | "hook" | "file" | "git",
  "observed_at": "<RFC3339-UTC>"
}
```

Wire encoding: one `data:` line per envelope, no extra whitespace; SSE `id:` field set to `event_id` so EventSource populates `Last-Event-ID` on auto-reconnect.

Schema versioning rules (per CLAUDE.md "schema_version + provenance" principle):
- Additive optional fields → no version bump on the wire (clients ignore unknown fields), but `schema_version` stays `"1"`.
- Breaking changes → new endpoint path `/v2/stream`, both versions coexist until clients migrate. No silent format change.

---

## 5. Resume protocol — `Last-Event-ID`, gap, resync

State machine for the SSE handler at connection start:

| Client signal | Server behavior |
|---|---|
| No `Last-Event-ID` header, no `?last_event_id=` | Backfill = nothing. Subscribe live. (Client is expected to have called GET separately to load baseline.) |
| `Last-Event-ID: <ulid>` matches a row | Backfill from that row exclusive, ordered ASC. Then live. |
| `Last-Event-ID: <ulid>` well-formed but no match | Emit one `event: resync\ndata: {"reason":"unknown_cursor"}\n\n`, backfill skipped, live forward begins. |
| `Last-Event-ID: <garbage>` | HTTP 400 (this *is* malformed input, not stale state). |

Mid-stream:

| Condition | Frame emitted | Client response |
|---|---|---|
| Normal envelope | `id: <ulid>\ndata: {...}\n\n` | Merge into UI state |
| `broadcast::Receiver::recv` returns `Lagged(n)` | `event: gap\ndata: {"dropped": n, "since": "<last_emitted_id>"}\n\n` | Re-issue baseline GET, clear local state, EventSource auto-reconnects with current `Last-Event-ID` |
| Idle for `keepalive_secs` | `:keepalive\n\n` (SSE comment, no event) | EventSource ignores; connection stays open |
| Server shutting down | Close TCP; EventSource auto-reconnects on next attempt | n/a |

`event: resync` semantics chosen over HTTP 400 because EventSource exposes 4xx only as `onerror` with no detail, so a client receiving 400 cannot distinguish "stale cursor" from "server down" and loops with the same cursor. The data-frame approach lets the client take a deliberate recovery path.

---

## 6. WebUI changes

### 6.1 `SessionListPage`

- Opens `EventSource('/v1/stream')` on mount; closes on unmount.
- New state: `envelopeAt: Map<SessionId, number>` (last envelope arrival time per session).
- On `message`: parse envelope, update or insert the matching row, set `envelopeAt[session_id] = Date.now()`.
- On `event: gap` / `event: resync`: re-call `GET /v1/sessions`, clear `envelopeAt`, keep the EventSource open.
- LIVE badge logic changes (v1 = slice-7, v2 = this slice):

  | Trigger | v1 (slice-7) | v2 (slice-8) |
  |---|---|---|
  | Data source | `now - last_observed_at < 60s` | `now - envelopeAt[sid] < 60s` |
  | Meaning | "DB row was recent the last time we fetched" | "this client is currently observing this session" |
  | Behavior when SSE drops 30s+ | unchanged (false-positive LIVE) | turns OFF (honest signal) |
  | First load before any envelope | possibly ON | OFF |

  The 5-second re-render ticker from slice-7 stays (it is still needed to recompute the "less-than-60s" predicate).

### 6.2 `SessionDetailPage`

- Opens `EventSource('/v1/stream?session=' + currentSessionId)` on mount; closes on unmount and on `session_id` change (route param).
- On `message`: append a Timeline marker built from envelope alone (lane derives from `kind` via existing `laneMapping.ts`). Envelope arrival does not trigger detail fetch.
- SourcePanel's lazy `GET /v1/events/:id/raw` stays as is — opens only on user click, unchanged from slice-2.
- Selection preservation: `selectedNodeId` lives in the page's own state; appending other markers does not reset it.
- Sort/scroll: Timeline is `event_id`-ordered; appending newer ids does not reorder older ones.

### 6.3 SSE client helper

A single `useLiveStream(query)` hook centralizes:
- EventSource lifecycle (mount/unmount/visibility/route-param-change)
- `event: gap` and `event: resync` callbacks
- Reconnection backoff (EventSource handles this, hook just exposes connection state for UI)

Both pages use the same hook. No duplicate EventSource bookkeeping per page.

### 6.4 Cursor persistence across reload

`EventSource` itself does not survive page reload, so the client must persist `Last-Event-ID` if it wants F5 to feel smooth.

- Storage: `sessionStorage[witmcc:cursor:<scope>]` where `<scope>` is either `"global"` (SessionListPage) or the `session_id` (SessionDetailPage).
- Write: on every received envelope, write `event_id` to the matching key.
- Read: on mount, pass stored cursor to EventSource via `?last_event_id=<id>` query param (EventSource cannot set arbitrary request headers; query param is the portable channel).
- Lifecycle: `sessionStorage` clears on tab close, survives reload. This matches the user's mental model of "F5 should feel continuous; closing and reopening starts fresh".
- `localStorage` rejected for MVP: a 24h-stale cursor would always trigger `event: resync`, defeating its purpose, and adds privacy surface (cursor in storage indefinitely) without UX benefit.

---

## 7. Test strategy

CLAUDE.md commit checklist applies to every commit in this slice. Failing tests are written first; production code follows.

### 7.1 Layer matrix

| Layer | Runner | Purpose |
|---|---|---|
| L1 | `cargo test --lib` | `LiveSink` trait, envelope serde, store-emits-on-commit unit tests |
| L2 | `cargo test --test sse_*` (axum-test, in-process) | Race window, dedup, gap, session filter, resume |
| L3 | `cargo test --test sse_subprocess` (real `witmcc serve` + reqwest SSE client) | One canonical scenario per ingest path (×7) + reconnect resume |
| L4 | `pnpm vitest` | EventSource lifecycle, state merging, badge semantics |
| L5 | claude-in-chrome manual smoke | Real browser, real `claude`, real network behavior |

### 7.2 L1 — cargo unit (red-first sequence)

| # | Test | Locks |
|---|---|---|
| U-1 | `livesink_broadcast_emits_envelope` | `LiveSink::Broadcast::emit` calls `Sender::send` |
| U-2 | `livesink_noop_does_not_panic_on_no_subscribers` | CLI `ingest --all` path is safe |
| U-3 | `livesink_capturing_records_in_order` | Test helper used downstream |
| U-4 | `store_observed_event_invokes_sink_on_commit` | sink fires after sqlx COMMIT |
| U-5 | `store_observed_event_skips_sink_on_error` | sink not invoked on INSERT failure |
| U-6 | `liveevent_envelope_v1_roundtrip` | JSON serde stable; future fields ignored |
| U-7 | `liveevent_from_each_event_kind` | All `EventKind` variants produce a valid envelope (strum::EnumIter enforces exhaustiveness at compile time) |

### 7.3 L2 — cargo integration (race + protocol)

| # | Test | Scenario |
|---|---|---|
| I-1 | `subscribe_before_select_no_loss` | One INSERT lands between subscribe and SELECT; client sees exactly one frame |
| I-2 | `subscribe_after_select_loses_event_regression` | Deliberately wrong order asserts loss — guards against future refactor inverting subscribe/SELECT |
| I-3 | `dedup_window_between_backfill_and_broadcast` | Same `event_id` appears in backfill and broadcast; client sees one |
| I-4 | `lagged_emits_gap_event` | `capacity=4` channel, burst 8 inserts, handler emits `event: gap` |
| I-5 | `gap_payload_includes_last_seen_cursor` | Gap data has `since` field set to last successfully forwarded id |
| I-6 | `session_filter_does_not_leak_other_sessions` | Two sessions writing concurrently; `?session=A` stream sees zero B envelopes |
| I-7 | `last_event_id_resume_from_middle` | LEI header → backfill only newer rows |
| I-8 | `last_event_id_at_latest_immediately_live` | LEI = latest id → zero backfill |
| I-9 | `last_event_id_unknown_emits_resync` | LEI is valid ULID but no row → `event: resync` first frame |
| I-10 | `last_event_id_malformed_returns_400` | Non-ULID → HTTP 400 (this *is* a client error) |
| I-11 | `client_disconnect_drops_subscriber` | Sender's `receiver_count()` decrements after handler exits |
| I-12 | `keepalive_comment_after_idle` | No envelopes for `keepalive_secs + 1` → exactly one `:keepalive` frame |
| I-13 | `empty_session_then_first_event` | Stream open before any INSERT, first INSERT after subscribe emits live |

### 7.4 L3 — cargo subprocess E2E (production wiring guard)

`tests/sse_subprocess.rs` spawns `witmcc serve --bind 127.0.0.1:0 --shutdown-after-ms 8000`. Each ingest path × 1 — these are the only tests that prove `LiveSink` is actually wired into the production code path, not silently `Noop`.

| # | Test | Path |
|---|---|---|
| E-1 | `binary_emits_sse_for_transcript_live_tail` | Tmp transcripts root, append one JSONL line → SSE frame |
| E-2 | `binary_emits_sse_for_otel_traces` | POST /otel/v1/traces with real slice-6 v01 fixture |
| E-3 | `binary_emits_sse_for_otel_metrics` | Same, metrics |
| E-4 | `binary_emits_sse_for_otel_logs` | Same, logs |
| E-5 | `binary_emits_sse_for_hooks` | POST /hooks/v1/events |
| E-6 | `binary_emits_sse_for_file_event` | Tmp watched dir, touch a file |
| E-7 | `binary_emits_sse_for_git_commit` | Tmp git repo, `git commit` |
| E-8 | `reconnect_with_last_event_id_no_loss` | Drop stream A mid-flight, new connection auto-sends `Last-Event-ID`, zero loss |
| E-9 | `concurrent_sessions_two_streams` | Two `?session=` subscribers, no cross-leak |
| E-10 | `shutdown_closes_streams_gracefully` | Streams close cleanly on `--shutdown-after-ms` |

### 7.5 L4 — vitest (client state)

EventSource is mocked via a `MockEventSource` helper. Real network behavior is L5's job.

| # | Test | Scenario |
|---|---|---|
| V-1 | `session_detail_opens_eventsource_on_mount` | URL includes `?session=` |
| V-2 | `envelope_appends_timeline_marker` | Mock dispatch → marker present in DOM |
| V-3 | `envelope_for_other_session_is_ignored_on_detail_page` | Defense-in-depth against server filter bug |
| V-4 | `gap_event_triggers_baseline_refetch` | `event: gap` → GET re-called |
| V-5 | `resync_event_clears_state_and_refetches` | `event: resync` → local state cleared |
| V-6 | `unmount_closes_eventsource` | `.close()` called |
| V-7 | `selected_node_preserved_during_live_update` | Selection survives new envelopes |
| V-8 | `live_badge_off_without_envelope` | New SessionListPage row with `last_observed_at = now-10s` but no envelope → badge OFF (v2 semantics) |
| V-9 | `live_badge_on_after_envelope` | `last_observed_at = 1h ago` + one envelope arrives → badge ON |
| V-10 | `live_badge_off_after_60s_silence` | Envelope arrived 70s ago → badge OFF |
| V-11 | `sort_state_preserved_on_session_list_update` | Sort by `event_count desc` survives a list update |
| V-12 | `multi_event_burst_no_state_thrash` | 100 envelopes in one tick → single render commit (React batching) |
| V-13 | `cursor_persisted_in_session_storage` | Envelope received → `sessionStorage` key set to `event_id` |
| V-14 | `cursor_read_on_mount_passed_as_query` | Pre-populated `sessionStorage` → EventSource URL includes `?last_event_id=<id>` |

### 7.6 L5 — claude-in-chrome smoke (manual, mandatory before each PR commit)

The commit checklist requires running the L5 items that touch the changed surface. Each commit message lists the smoke items run.

| # | Scenario | Why only L5 catches it |
|---|---|---|
| S-1 | Real `claude` CLI running. Open SessionDetailPage. Type prompt. New user/assistant nodes appear within ~2s. | End-to-end through real transcript writer + notify + broadcast + EventSource |
| S-2 | Real `claude` calls a tool. SessionDetail gets new OTel marker live. | OTel POST path through to SSE |
| S-3 | Real PreToolUse hook fires. Hook lane marker appears live. | Hook POST path |
| S-4 | Open SessionDetailPage, switch tabs for 5 minutes, return. Stream resumes, no loss. | Chrome background-tab throttling does not pause SSE push (verify) |
| S-5 | Hard reload (F5). Page reopens; new EventSource backfills from last seen `event_id` (stored in `sessionStorage`). | EventSource state is forgotten on reload; client must persist cursor |
| S-6 | DevTools → offline 30s → online. EventSource auto-reconnects with `Last-Event-ID`. | Browser-managed reconnect |
| S-7 | Navigate away and back via SPA. EventSource closes and reopens cleanly. | React lifecycle vs EventSource lifecycle |
| S-8 | Two tabs of the same `?session=` URL. Both receive live updates. | Browser per-origin connection budget (~6) holds |
| S-9 | Two tabs of different `?session=` URLs. No cross-leak in the DOM. | Visual confirmation of I-6 |
| S-10 | SessionListPage open. Start a new `claude` conversation. New row appears at top. | Verifies global stream + insert-on-unknown-session-id logic |
| S-11 | 30 min idle. Connection still alive; next envelope arrives. | Keepalive interval correct against host proxies/firewalls |

### 7.7 Silent-regression guards — explicit

These four guards exist because the listed risks would be invisible to the rest of the test suite.

| Guard | Risk it closes |
|---|---|
| **L3 E-1..E-7 (one per ingest path)** | "Production code passes `LiveSink::Noop` by accident" — L1/L2 all use sinks directly so they cannot catch wiring mistakes |
| **V-8..V-10 (badge semantics)** | LIVE badge could keep passing slice-7 tests after switching to envelope-based logic; explicit assertions on the new semantic prevent silent rollback |
| **U-7 with `strum::EnumIter`** | New `EventKind` added later without updating `LiveEvent::from_observed` → compile error rather than silent kind=Unknown |
| **`tests/ingest_store.rs::*_idempotent` extended with `CapturingSink`** | `ingest_file` called twice on the same fixture: sqlite dedupes (existing), sink emits zero on the second call (new) — prevents SSE-side duplicates on transcript rewrites |

---

## 8. Migration order (TDD red-first, no half-state per commit)

Each numbered step is one commit. The invariant after step 4: all existing cargo tests still pass.

```
1.  red:   U-6, U-7 (envelope serde + EnumIter exhaustiveness)
2.  green: LiveEvent struct + LiveSink trait + Noop/Capturing/Broadcast impls
3.  red:   U-1..U-5 (LiveSink behavior)
4.  green: store_observed_event/store_commit/store_file_event/etc gain &dyn LiveSink;
           every existing call site passes &LiveSink::Noop. Existing tests must
           still pass — this is the migration-correctness checkpoint.
5.  red:   Guards extended on existing idempotency tests (CapturingSink dedup)
6.  green: Sink-aware dedup paths in store_commit / store_file_event already
           correct from step 4; this step just exposes them.
7.  red:   L3 E-1..E-7 (production wiring per ingest path)
8.  green: transcript_tail / watcher / git_poller / api::otel / api::hook
           wired with AppState.live_tx; CLI ingest path stays Noop.
9.  red:   L2 I-1..I-13 (race, gap, resync, filter, keepalive)
10. green: SSE handler implementation
11. red:   L4 V-1..V-12 (client state + badge)
12. green: useLiveStream hook + SessionListPage + SessionDetailPage updates
13. red:   L3 E-8..E-10 (subprocess reconnect + concurrent sessions)
14. green: Whatever wiring fixes remain
15. L5:    Run S-1..S-11. Record passing items in commit message of step 14
           or a subsequent docs commit. Open PR.
```

If step 4 turns any existing test red, it is a migration bug — do not move on.

---

## 9. Configuration

CLI flags added to `witmcc serve`:

```
--sse-keepalive-secs <SECS>   default: 30, range: 5..=120
--sse-channel-capacity <N>    default: 512, range: 64..=8192
```

No env-var equivalents (keeps surface small; the CLI is the only place this is configured).

`AppState` gains:

```rust
pub struct AppState {
    pub pool: SqlitePool,
    pub live_tx: Arc<broadcast::Sender<LiveEvent>>,
    pub sse_keepalive_secs: u64,
    // existing fields ...
}
```

`router` becomes `pub fn router(state: AppState) -> Router` (state struct, not loose args) — this is the only signature change that ripples through tests, and it ripples through *every* existing cargo test that constructs the router. The simplification pays for itself once and for all.

---

## 10. Open decisions resolved

| # | Decision | Resolution |
|---|---|---|
| 1 | Endpoint shape | `/v1/stream` + optional `?session=<id>` server-side filter (hybrid c′) |
| 2 | Unknown `Last-Event-ID` | `event: resync` frame, not HTTP 400. (400 reserved for malformed ULID — actual client error) |
| 3 | L5 smoke cadence | Mandatory before each PR commit; passed items listed in commit message |
| 4 | Keepalive interval | 30s default, `--sse-keepalive-secs 5..=120` |
| 5 | LIVE badge semantic | "60s since last envelope received" (client observation), not "60s since last_observed_at" (DB freshness) |

---

## 11. Non-goals and explicit deferrals (CLAUDE.md anchor)

This slice does **not** introduce any of:

- Patch / install / Claude Code config mutation (CLAUDE.md non-goal)
- Annotation, label, or status writes from external clients (CLAUDE.md non-goal)
- Cross-session pattern detection (M5+)
- Findings engine — `Finding` / `RootCauseHypothesis` / `QualitySummary` (M5)
- Token / Origin enforcement beyond the existing slice-1 host_allowlist (M7)
- `0.0.0.0` bind opt-in (slice-1 setting, unchanged)
- WebUI redaction preview (M7)

Documentation deferrals: same as slice-6 DEV-S6-09 / slice-7 DEV-S7-08. `docs/02_technical_architecture_spec.html` and `docs/04_api_mcp_spec.html` carry single-line HTML that is unsafe to patch inline; the design spec + `docs/implementation-notes.html` continue to act as the source of truth for now. A docs-only consolidation slice will fold these in later.

---

## 12. Acceptance criteria

1. **No-refresh smoothness.** With `claude` running and a witmcc WebUI tab open on either page, every new ingested event appears in the DOM within 2 seconds of being persisted, with no user action. Verified by S-1, S-2, S-3, S-10.
2. **Resume correctness.** A client disconnected for any duration up to the SSE channel's effective retention reconnects without loss or duplication. Verified by E-8, I-7.
3. **Honest LIVE badge.** LIVE turns OFF within 60s of stream silence; turns ON within 60s of envelope arrival. Verified by V-8, V-9, V-10.
4. **No silent live-emission regression.** Every ingest path has a subprocess test proving sink wiring. Verified by E-1..E-7.
5. **No-op migration safety.** Every cargo test that existed before slice-8 still passes at step 4 of §8 (sink-aware, Noop-injected). Migration correctness checkpoint.
6. **MCP-readiness.** The same `Arc<broadcast::Sender<LiveEvent>>` is exposed in `AppState` and consumed by exactly one handler today; M6 can add an MCP handler without touching ingest code.

---

## 13. Risks and how the spec closes each

| Risk (from prior brainstorming) | Mitigation in this design |
|---|---|
| Subscribe ↔ SELECT race | §3 invariant 1, I-1, I-2 (both directions tested) |
| SSE test flake | L3 subprocess uses real binary + bounded `--shutdown-after-ms`; L4 mocks EventSource entirely; no `setTimeout` in any assertion path |
| Lagged backpressure | `event: gap` + `event: resync` paths; I-4, I-5, V-4 |
| Signature ripple breaks existing tests | §8 step 4 makes "tests still green" the migration checkpoint; `router(state)` consolidation pays the cost once |
| LIVE badge silent regression | V-8..V-10 lock new semantic explicitly |
| Browser-only timing surprises | L5 S-4, S-5, S-6, S-7, S-11 enumerate them — manual smoke, but explicit |

---

## 14. References

- `docs/06_mvp_execution_plan.html` §M6 "streaming status events"
- `docs/02_technical_architecture_spec.html` §pipeline (in-process model)
- `CLAUDE.md` — TDD red-first, real-data anchoring, UI browser smoke before commit
- W3C HTML Living Standard, *Server-sent events* (Last-Event-ID, retry, comment frames)
- `tokio::sync::broadcast` docs — `Lagged` semantics, capacity tradeoffs
- slice-7 implementation-notes — `transcript_tail::run` ingest_file reuse pattern (precedent for sink threading)
