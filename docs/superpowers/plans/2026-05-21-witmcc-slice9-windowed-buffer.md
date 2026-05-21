# Slice-9 Implementation Plan — Windowed Event Buffering

**Spec:** `docs/superpowers/specs/2026-05-21-witmcc-slice9-windowed-buffer-design.md`
**Branch:** `slice9-windowed-buffer`
**Strategy:** TDD red-first per task. Each task starts with a failing test. No task is "done" until its test is green AND prior tasks are still green.

---

## Phase 1 — Server cursor + range query (L1 + L2)

| # | Task | Test (red-first) | Impl |
|---|---|---|---|
| 1 | `Cursor` type with parse/format | `tests/cursor_test.rs` (or inline `#[cfg(test)] mod`): `parse("2026-05-21T11:42:33.012Z\|01J...")` → `Cursor { observed_at, event_id }`; `format()` → original. Round-trip empty / invalid → `Err`. | `src/model/cursor.rs` — `#[derive(Debug, PartialEq, Eq)] struct Cursor { observed_at: DateTime<Utc>, event_id: String }`. `FromStr` + `Display`. |
| 2 | `repo_observed::list_session_window` no-cursor case | L1: 1000 rows seeded, `list_session_window(pool, sid, None, None, 500)` → returns 500 newest, ASC ordered. | `src/db/repo_observed.rs` — new fn, DESC LIMIT + reverse. |
| 3 | `list_session_window` `before` only | L1: cursor at row[500], `before=Some(c)` → returns rows[0..500] ASC. | SQL: `WHERE (observed_at, event_id) < (?, ?) ORDER BY ... DESC LIMIT ?` then reverse. |
| 4 | `list_session_window` `after` only | L1: cursor at row[500] → returns rows[501..1000] ASC. | SQL: `WHERE ... > (?, ?) ORDER BY ... ASC LIMIT ?`. |
| 5 | `list_session_window` both cursors | L1: returns slice between cursors. | Combined predicate. |
| 6 | `list_session_window` limit clamp | L1: `limit=5000` clamps to 1000. | Inside fn. |
| 7 | Cross-source ordering | L1 regression: rows with mixed `metric:...` and ULID event_ids — ordering matches DEV-S8-10 (observed_at primary). | SQL already uses observed_at primary; assertion lock. |
| 8 | `GET /v1/sessions/:id/events` handler — green-path | L2 (`tests/api.rs`): seed 1000 events, hit endpoint without cursors, response `data.events.length == 500`, `prev_cursor` non-null, `next_cursor` either null or = newest event cursor. | `src/api/routes.rs::session_events` + route in `src/api/mod.rs`. |
| 9 | `?before=` paging | L2: hit endpoint twice with second call using first response's `prev_cursor` as `?before=`. Combine events, assert no duplicates + no gaps + correct ordering. | (handler already supports — assertion only) |
| 10 | `?after=` paging | L2: same but forward direction. | (handler) |
| 11 | Cursor parse errors | L2: `?before=garbage` → 400 with descriptive error. | Handler error mapping. |
| 12 | `GET /v1/sessions/:id` drops `events` | L2: existing `session_detail_and_graph` test red, summary still populated. **Edit existing test in the same commit.** | `src/api/dto.rs::SessionDetail` — remove `events`. `src/api/routes.rs::session_detail` — only summary. |

**Commit 1:** Phase 1 — server range query + cursor + summary-only session_detail.

---

## Phase 2 — `rebuild_session` atomicity (L2)

| # | Task | Test (red-first) | Impl |
|---|---|---|---|
| 13 | Parallel SELECT chaos | L2 (`tests/graph_atomicity.rs`): seed session with 1000 graph_nodes. Spawn one task running `rebuild_session` 20 times. Spawn 20 tasks each doing 50 SELECTs against `graph_node WHERE session_id=?`. Assert: every SELECT returns either pre-state-row-count or post-state-row-count, never 0 (when both states are non-zero). | `src/graph/build.rs::rebuild_session` — `let mut tx = pool.begin().await?; ... tx.commit().await?`. Compute graph rows OUTSIDE the tx (read from pool); only DELETE + INSERT inside tx. `repo_graph::insert_node_tx` / `insert_edge_tx` variants. |

**Commit 2:** Phase 2 — rebuild_session transaction + chaos test.

---

## Phase 3 — Client viewport hook (vitest)

| # | Task | Test (red-first) | Impl |
|---|---|---|---|
| 14 | `useSessionWindow` initial fetch | vitest (`webui/src/hooks/__tests__/useSessionWindow.test.tsx`): mount hook with mocked fetch → events populated, `atLiveTip=true`, loading transitions `initial`→`idle`. | `webui/src/hooks/useSessionWindow.ts`. |
| 15 | `loadOlder` prepend | vitest: after initial fetch, call `loadOlder()`, mocked response returns older window, events array length grows, oldest cursor updates. | Hook. |
| 16 | `appendOne` newer event | vitest: append observed_at > newestCursor → array grows, newestCursor advances. | Hook. |
| 17 | `appendOne` older / duplicate | vitest: append observed_at <= newestCursor or duplicate event_id → ignored. | Hook. |
| 18 | LRU cap | vitest: append 5001 events → oldest 500 dropped, oldestCursor cleared. | Hook. |
| 19 | Client API helpers | vitest (`webui/src/api/__tests__/client.test.ts`): `getSessionEvents(id, {before, after, limit})` builds correct URL, parses cursor in response. | `webui/src/api/client.ts` — new fn. |

**Commit 3:** Phase 3 — useSessionWindow + client helpers.

---

## Phase 4 — SessionDetailPage rewiring (vitest + manual)

| # | Task | Test (red-first) | Impl |
|---|---|---|---|
| 20 | Page mount fetches summary+graph+events | vitest (`webui/src/routes/__tests__/SessionDetailPage.test.tsx`): mount with mocked endpoints → assert getSession, getGraph, getSessionEvents each called once. | `webui/src/routes/SessionDetailPage.tsx` — rewire. |
| 21 | Envelope drives append, not refetch | vitest: dispatch envelope via onEnvelope → assert getSessionEvents called with `after=newestCursor`, NOT getSession + getGraph. | SessionDetailPage onEnvelope handler. |
| 22 | Graph re-fetch threshold | vitest: 49 envelopes → getGraph not re-called; 50th envelope OR 10s timer → getGraph called once. | useEffect timer + envelope counter. |
| 23 | IntersectionObserver triggers loadOlder | vitest: render with mocked IO entry intersection=1 → loadOlder invoked. | sentinel div + IO setup. |
| 24 | Remove debounce + identity-skip | vitest: existing DEV-S8-13 assertions removed, new ones assert immediate setState on envelope. | Delete debounce ref + identity-skip block. Existing tests that assert debounce → update or remove. |

**Commit 4:** Phase 4 — SessionDetailPage envelope-driven append.

---

## Phase 5 — L3 subprocess + browser smoke

| # | Task | Test (red-first) | Impl |
|---|---|---|---|
| 25 | 10000-event paging | L3 (`tests/sse_subprocess.rs` extension or new `tests/events_subprocess.rs`): seed 10000 events via JSONL ingest, hit `/v1/sessions/:id/events?before=...` 20 times, assert union == 10000 unique + strict ordering. | (handler already covers — test only) |
| 26 | Live tip after paging | L3: same 10000 set, page back 5 times, then POST 100 hook events, hit `?after=<initial newest>&limit=1000` → returns the 100 new. | (test only) |
| 27 | claude-in-chrome smoke S-1 | manual: open SessionDetailPage on 9000+ event real session, scroll back ≥5 pages, screenshot. | (smoke only) |
| 28 | claude-in-chrome smoke S-2 | manual: trigger real claude Bash → envelope arrives, Timeline appends in <2s no flicker. | (smoke only) |
| 29 | claude-in-chrome smoke S-3 | manual: F5 reload, summary restored, latest 500 visible, scroll-back still works. | (smoke only) |

**Commit 5:** Phase 5 — L3 + smoke records.

---

## Phase 6 — Docs + PR

| # | Task | Detail |
|---|---|---|
| 30 | implementation-notes slice-9 section | Add DEV-S9-01..NN. Mark DEV-S8-12, DEV-S8-13, DEV-S8-14 as **superseded by slice-9**. List specific commits. |
| 31 | CLAUDE.md status update | "현재 단계: **M3·M4 진행 중** (slice-1~9 완료)". |
| 32 | Open PR | `gh pr create` with summary linking spec/plan/implementation-notes. |

**Commit 6:** docs + notes.

---

## Done definition

- All 32 tasks green.
- `cargo test`, `cd webui && npm test`, `cargo clippy -- -D warnings` pass.
- claude-in-chrome smoke 3 scenarios documented in implementation-notes with screenshot refs.
- PR opened from `slice9-windowed-buffer` → `main`.
- No DEV-S8-12 / DEV-S8-13 / DEV-S8-14 references survive in active code.

---

## Risks tracked from spec

- R1 cursor precision — Phase 1 task 1 round-trip test.
- R2 IntersectionObserver — Phase 4 task 23.
- R3 envelope/fetch race dedup — Phase 3 task 17.
- R4 transaction lock window — Phase 2 task 13 measures via timing assertion if needed; compute outside tx mitigates.
