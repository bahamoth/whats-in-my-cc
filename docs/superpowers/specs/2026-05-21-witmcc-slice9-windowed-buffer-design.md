# Slice-9 Design — Windowed Event Buffering (server range query + client chunk cache)

**Date:** 2026-05-21
**Branch:** `slice9-windowed-buffer` (based on `main` post slice-8 PR #6 merge)
**Goal:** Replace slice-8's `?limit=5000` newest-N cap and refetch-the-world live update with a video-streaming style range-query API + client-side chunk cache + envelope-driven incremental append. Settles DEV-S8-12 (rebuild_session race), DEV-S8-13 (renderer freeze under high envelope rate), DEV-S8-14 (oldest-N visibility bug) — each one currently has a workaround flagged for slice-9.

---

## 1. Motivation

Three slice-8 deviations all point at the same shape:

- **DEV-S8-14** — `GET /v1/sessions/:id` returns newest 5000 events. Sessions with 5000+ events can never page back to older events from the WebUI.
- **DEV-S8-13** — Every SSE envelope re-fetches the whole 5000-event JSON + re-runs the Timeline layout over thousands of SVG circles. We band-aided this with a 1000ms trailing debounce and a setState identity-skip; the underlying refetch-the-world pattern remains.
- **DEV-S8-12** — `graph::build::rebuild_session` runs DELETE+INSERT without a transaction, so SELECTs that race the rebuild see an empty graph. We band-aided this server-side (200 with empty graph) and client-side (keep previous graph on empty). The race still exists.

slice-9 fixes the root cause for all three by switching the read model from "give me the latest 5000 events" to "give me events from cursor A to cursor B, append future events as they arrive". This matches how a video player loads media: pre-buffer the visible window, fetch more as the user scrolls, never reload the file.

---

## 2. Scope

### In scope

- **Server range query endpoint** — `GET /v1/sessions/:id/events?before=&after=&limit=`. Returns an ordered window of events with `prev_cursor` / `next_cursor` for paging. Stateless.
- **`session_detail` becomes summary-only** — `GET /v1/sessions/:id` drops the `events: [...]` window. Returns `{session_id, summary}` only. The webui calls `/events?...` separately. (Existing test assertions migrate in the same commit.)
- **`rebuild_session` transaction** — wrap DELETE+INSERT in `sqlx::transaction`. Concurrent SELECTs see either the old or the new state, never partial.
- **Client viewport state + chunk cache**
  - `useSessionWindow(sessionId)` hook owns `events: ObservedEvent[]` sorted by `(observed_at, event_id)`, plus `oldest_loaded_cursor`, `newest_loaded_cursor`.
  - Initial fetch: newest 500 events. Scroll near top (Timeline's left edge in view) → fetch older window via `?before=<oldest>&limit=500`. Append/prepend without re-fetching what's already loaded.
  - LRU cap: hard limit `MAX_EVENTS_IN_WINDOW = 5000`. When exceeded by appending the newer end, drop the oldest 500 (whichever end is opposite the recent activity). The dropped cursor remains paginable via `?before=` on demand.
- **SSE envelope drives append, not refetch**
  - On envelope with `observed_at > newest_loaded_cursor`: ask `/v1/events/:id/raw` for the full event row, append to the window, advance `newest_loaded_cursor`. No more `getSession` + `getGraph` re-fetch on every envelope.
  - On envelope with `session_id` matching but `observed_at <= newest_loaded_cursor`: ignore (we already have this row via initial fetch or via the previous envelope).
  - Timeline re-renders with the new full array; React's identity-skip on the Timeline child keeps the SVG layout incremental (existing logic from DEV-S8-13 part 2 stays).
- **Graph refresh stays summary-driven** — graph is still fetched on session open, but envelope no longer triggers `getGraph`. Instead: when summary's `event_count` advances beyond the last graph fetch by a configurable threshold (default 50), re-fetch the graph once. This breaks the "every envelope is a graph rebuild" cycle while keeping the graph eventually-consistent.
- **Test coverage**
  - L1: range query returns expected window for empty / single-page / multi-page / with-before / with-after / limit-clamp / no-results-after-last.
  - L2: `rebuild_session` is atomic — spawn N parallel SELECTs while rebuild runs; none observe empty graph_node when source rows exist.
  - L3 subprocess: 10000-event session, page through windows, total events delivered = COUNT(*).
  - vitest: useSessionWindow chunk cache (hit/miss, prepend, append, LRU eviction).
  - claude-in-chrome browser smoke: 9000+ event real session, scroll back ≥ 5 pages, live envelope drives append (no whole-page reload).

### Out of scope

- **react-window / virtualized Timeline** — Timeline is SVG-based. Current cap (5000 nodes) renders fine in Chrome on bahamoth's machine. Real virtualization is a slice-10+ optimization, not slice-9.
- **Backward search ("jump to event_id at observed_at X")** — slice-10 deep-link work. slice-9 only handles "load older" via scroll, not arbitrary seek.
- **Per-kind viewport filtering** (`?kinds=tool_call,tool_result`) — slice-11 (insights surface) needs this; slice-9 leaves it out to keep the cursor surface small.
- **Cross-session windowing** — `GET /v1/sessions/:id/events` is the slice-9 surface. `GET /v1/events?from=&to=&kind=` is a future MCP read-resource shape.

---

## 3. API Contract

### `GET /v1/sessions/{session_id}/events`

Query parameters (all optional):

| Param | Type | Default | Meaning |
|---|---|---|---|
| `before` | `<observed_at>\|<event_id>` cursor | — | Return events strictly *older* than this cursor. Same shape as `next_cursor` returned by the API. |
| `after` | `<observed_at>\|<event_id>` cursor | — | Return events strictly *newer* than this cursor. |
| `limit` | int | 500 | Clamp to `[1, 1000]`. |

Behavior:

1. **No `before` / `after`** — return newest `limit` events, ordered ASC by `(observed_at, event_id)`. Response `prev_cursor` points before the oldest row returned, `next_cursor` is `null` (you're at the live tip).
2. **`before` only** — events strictly older than the cursor, newest-`limit` of them, ordered ASC. `prev_cursor` is the cursor of the oldest row returned, `next_cursor` is the cursor of the newest. Empty result → both null.
3. **`after` only** — events strictly newer, oldest-`limit` of them, ordered ASC. Symmetric.
4. **Both `before` and `after`** — events with `(observed_at, event_id)` strictly between the two cursors. Up to `limit` of them, ordered ASC. Useful for seek but not in slice-9 client.

Response:

```json
{
  "data": {
    "events": [ { ...ObservedEvent... }, ... ],
    "prev_cursor": "2026-05-21T11:42:33.012Z|01J...",
    "next_cursor": "2026-05-21T11:42:34.001Z|01J..." | null
  },
  "meta": { "schema_version": "0.5.0", ... }
}
```

Cursor format: `<observed_at_rfc3339>|<event_id>`. URL-encode the `|` when passing through query strings. Same `(observed_at, event_id)` ordering as the SSE backfill (DEV-S8-10).

Errors:

- `400` — cursor doesn't parse, or `before`/`after` ordering inconsistent.
- `404` — session has zero rows in `observed_event` (the existing `session_summary` lookup decides this — keeps slice-8 behavior).

### `GET /v1/sessions/{session_id}` (changed)

Drop the `events` array. Response data becomes:

```json
{
  "data": {
    "session_id": "sess-A",
    "summary": {
      "event_count": 12345,
      "by_kind": { ... },
      "first_observed_at": "...",
      "last_observed_at": "..."
    }
  }
}
```

Old callers that consumed `data.events` are updated in the same commit.

---

## 4. Server Implementation

### 4.1 `repo_observed::list_session_window`

New function:

```rust
pub async fn list_session_window(
    pool: &SqlitePool,
    session_id: &str,
    before: Option<&Cursor>,
    after: Option<&Cursor>,
    limit: i64,
) -> Result<Vec<ObservedEvent>>;
```

SQL composition (dynamic predicates, all-ASC final order):

- No cursor: `WHERE session_id = ? ORDER BY observed_at DESC, event_id DESC LIMIT ?` → reverse in memory.
- `before` only: `WHERE session_id = ? AND (observed_at, event_id) < (?, ?) ORDER BY observed_at DESC, event_id DESC LIMIT ?` → reverse.
- `after` only: `WHERE session_id = ? AND (observed_at, event_id) > (?, ?) ORDER BY observed_at ASC, event_id ASC LIMIT ?`.
- Both: `... > (?, ?) AND ... < (?, ?) ORDER BY observed_at ASC, event_id ASC LIMIT ?`.

`list_session` (old ASC variant) becomes dead code; remove. `list_session_latest` (slice-8 newest-N) becomes the no-cursor path of `list_session_window`; keep as a thin wrapper for the SSE backfill which already uses it.

`session_summary` (slice-8) stays.

### 4.2 `rebuild_session` transaction

Current `graph::build::rebuild_session(pool, session_id)`:

```rust
sqlx::query("DELETE FROM graph_node WHERE session_id = ?").execute(pool).await?;
sqlx::query("DELETE FROM graph_edge WHERE session_id = ?").execute(pool).await?;
// ...compute...
for n in nodes { insert(pool, n).await?; }
for e in edges { insert(pool, e).await?; }
```

becomes:

```rust
let mut tx = pool.begin().await?;
sqlx::query("DELETE FROM graph_node WHERE session_id = ?").execute(&mut *tx).await?;
sqlx::query("DELETE FROM graph_edge WHERE session_id = ?").execute(&mut *tx).await?;
// compute (read from observed_event via pool, not tx — observation isn't part of write set)
for n in nodes { insert_with(&mut *tx, n).await?; }
for e in edges { insert_with(&mut *tx, e).await?; }
tx.commit().await?;
```

SQLite's default WAL mode gives readers a consistent snapshot for the duration of the SELECT; a SELECT issued mid-transaction sees the pre-DELETE rows until commit. Concurrent SELECTs from `session_graph` handler observe either old or new graph, never empty.

`insert_with` is a tiny generic helper because `sqlx::query!` doesn't take an `Executor` directly; the existing `repo_graph::insert_node` / `insert_edge` signatures stay (callers pass `&pool` from the handler path), and new `insert_node_tx` / `insert_edge_tx` variants take a transaction.

### 4.3 Handler

`pub async fn session_events(State, Path, Query) -> ...` in `src/api/routes.rs`. Parse cursors, call `list_session_window`, project to DTO (reuse `observed_to_dto`), build response with `prev_cursor` / `next_cursor`.

Cursors for the response:
- Result empty → both null.
- Result non-empty:
  - `prev_cursor` = `format!("{}|{}", first.observed_at.to_rfc3339(), first.event_id)` (consumer of `?before=` paging older).
  - `next_cursor` = `format!("{}|{}", last.observed_at.to_rfc3339(), last.event_id)` (consumer of `?after=` paging newer). If the result already includes the session's live tip (last row's `(observed_at, event_id) == session_summary.max`), `next_cursor = null` — the SSE stream supersedes.

---

## 5. Client Implementation

### 5.1 `useSessionWindow` hook

```ts
function useSessionWindow(sessionId: string): {
  events: ObservedEvent[];   // chronological ASC
  oldest: Cursor | null;     // null = at session start
  newest: Cursor | null;     // null = at live tip
  loading: 'initial' | 'older' | 'newer' | 'idle';
  loadOlder: () => Promise<void>;
  appendOne: (e: ObservedEvent) => void;
};
```

Internal state:

- `events: ObservedEvent[]` — single contiguous array, sorted.
- `oldestCursor`, `newestCursor`, `atLiveTip: boolean`.
- `MAX_EVENTS_IN_WINDOW = 5000`. After append, if `events.length > MAX`, drop oldest 500 and clear `oldestCursor` (forces re-fetch on next `loadOlder`). LRU is one-sided because the active end is newer (live tip); user only paginates back by explicit scroll.

`loadOlder()` — issues `GET /v1/sessions/:id/events?before=<oldestCursor>&limit=500`, prepends, updates `oldestCursor` to the response `prev_cursor`. No-op if `loading !== 'idle'`. No-op if `oldestCursor === null` (page-1 fetch).

Initial mount — issues `GET /v1/sessions/:id/events?limit=500` (no cursors → newest 500). Sets `atLiveTip = true`, `newestCursor = next_cursor` (server returns `null` for live tip; we treat that as "tail follows SSE").

`appendOne(e)` — if `e.observed_at < newestCursor.observed_at` (or equal with smaller event_id), drop (already have it / out of order). Else push; advance `newestCursor`; trim oldest if over cap.

### 5.2 `SessionDetailPage` rewiring

Replace existing `refetch()` callback with:

- One-shot fetches on mount: `getSession(sessionId)` (summary) + `getGraph(sessionId)` + `getSessionEvents(sessionId)` (newest 500).
- SSE envelope handler:
  - `useSessionWindow.appendOne` — needs the full ObservedEvent, so envelope-handler issues `GET /v1/events/:event_id/raw` and projects to ObservedEvent. (Or simpler: SSE backfill already carries enough fields for the marker; for slice-9 we just call `getSessionEvents(sessionId, {after: newestCursor, limit: 50})` — a tiny incremental fetch.)
  - On every 50 events appended OR every 10 seconds, re-fetch `getGraph(sessionId)`. (Stale graph for a few seconds is acceptable; envelope-per-rebuild was the freeze cause.)
- Remove the 1000ms debounce + the setState identity-skip (DEV-S8-13). They were workarounds for refetch-the-world; window+append doesn't need them.

### 5.3 `Timeline` component (untouched)

Receives the events array unchanged. Identity stability of unchanged events comes for free because `useSessionWindow` pushes new objects only at the tip; existing array indices keep the same references.

### 5.4 Scroll trigger for `loadOlder`

`SessionDetailPage` mounts an `IntersectionObserver` on a sentinel `<div>` at the Timeline's left edge. When intersection ≥ 0.5 and `oldestCursor !== null` and `loading === 'idle'`, fire `loadOlder()`. Standard infinite-scroll pattern.

---

## 6. Test Plan

### L1 (Rust unit)

- `repo_observed::list_session_window` —
  - empty session → empty Vec
  - 1000 rows, no cursor, limit=500 → returns rows[500..1000] ASC
  - 1000 rows, before=row[500] cursor → returns rows[0..500] ASC
  - 1000 rows, after=row[500] cursor → returns rows[501..1000] ASC
  - limit=2000 clamps to 1000
  - cross-source event_id (`metric:...` vs ULID) ordering respects `observed_at` primary key (regression for DEV-S8-10).

### L2 (cargo integration)

- `GET /v1/sessions/:id/events` returns the expected window for each of the four parameter combinations.
- `GET /v1/sessions/:id` no longer returns `events` field (assertion: field absent or empty + summary populated).
- Cursor format round-trip: take `next_cursor` from response 1, pass as `?after=` to response 2 → no overlap, no gaps.
- `rebuild_session` atomicity: spawn 20 parallel `SELECT * FROM graph_node WHERE session_id=?` while `rebuild_session` runs in another task. Assert: every observation either matches the pre-state row count or the post-state row count. No observation sees 0 rows when both states have rows.

### L3 (subprocess E2E)

- 10000-event seed session. Page through with `?before=` 20 times. Assert: union of all pages == COUNT(*), no duplicates, ordering is strict ASC by `(observed_at, event_id)`.
- During paging, fire 100 hook POSTs to advance the live tip. Assert: a final `?after=<initial_newest>&limit=1000` returns those 100 events.

### vitest

- `useSessionWindow` —
  - initial fetch populates events + sets `atLiveTip = true`.
  - `loadOlder` prepends and updates `oldestCursor`.
  - `appendOne` with newer event pushes; with older event drops.
  - Cap: appending past 5000 drops oldest 500 and clears `oldestCursor`.
- `SessionDetailPage` —
  - mount fetches summary + graph + events once; envelope arrival appends without re-fetch of summary/graph.
  - Graph re-fetch fires after 50 envelopes (or 10s timer); not on every envelope.

### Browser smoke (claude-in-chrome)

- Load 9000+ event session (bahamoth's `ed82aee9-62a7-4cfe-bf9a-565379765b1e`). Initial Timeline renders newest 500. Scroll back → IntersectionObserver fires, older window appears. Repeat 5× without page reload.
- Trigger live activity (Bash command in active claude session). Envelope arrives → Timeline appends marker. No 404 flicker, no freeze, no debounce-introduced delay.
- F5 reload mid-session. Page restores: summary fresh, latest 500 visible, SSE cursor in `sessionStorage` honored.

---

## 7. Migration order (TDD red-first)

1. **L1 test for `list_session_window`** (red — function doesn't exist).
2. Implement `list_session_window`. Pass.
3. **L1 test for cursor format parse/format** (red). Implement `Cursor` type. Pass.
4. **L2 test for `GET /v1/sessions/:id/events`** (red — endpoint 404). Add handler + route. Pass.
5. **L2 test for `GET /v1/sessions/:id` no longer returns events** (red — current handler returns events). Update handler + DTO. Pass. (At this point every existing test that asserts on `data.events` is failing → migrate them in the same commit.)
6. **L2 test for rebuild_session atomicity** (red — current behavior shows 0-row observation). Wrap in transaction. Pass.
7. **vitest red for useSessionWindow initial fetch** (red — hook doesn't exist). Implement. Pass.
8. **vitest red for `loadOlder` + `appendOne`** (red). Implement. Pass.
9. **vitest red for SessionDetailPage SSE-driven append** (red — current page calls refetch). Rewire. Pass. Existing slice-7 LIVE badge test must still pass.
10. **L3 subprocess for 10000-event paging**. Add fixture + assertions.
11. **Browser smoke** (claude-in-chrome). Verify on real session. Capture 3 screenshots: initial load, scroll-back, live-append.
12. **implementation-notes update** (DEV-S9-01..NN) + CLAUDE.md status update.

---

## 8. Key Decisions (locked)

| # | Decision | Reason |
|---|---|---|
| 1 | Cursor is `<observed_at>\|<event_id>` not opaque base64 | Debuggable in browser network tab; clients can construct cursors from raw event rows. |
| 2 | `MAX_EVENTS_IN_WINDOW = 5000` (client) | Same heuristic as DEV-S8-14 but now self-correcting: drops trigger pagination, don't lose data. |
| 3 | Graph re-fetch every 50 envelopes or 10s | Empirically the freeze in DEV-S8-13 came from graph rebuild thrashing, not event marker drawing. Decoupling them is the actual fix. |
| 4 | `session_detail` drops `events` field (breaking) | The slice-8 `?limit=5000` cap on this endpoint was the bug. A summary-only endpoint is the correct shape. |
| 5 | `rebuild_session` reads from pool, writes via tx | Reads don't need to be in the tx (observation rows aren't part of write set); avoids holding write lock during compute. |

---

## 9. Risks

- **R1 — Cursor encoding edge cases.** `observed_at` may have ms precision differences between SQLite storage and Rust rendering. **Mitigation:** L1 round-trip test (parse → format → parse equal). L2 cross-source ordering test (regression for DEV-S8-10).
- **R2 — `IntersectionObserver` doesn't fire on tiny SVG layouts.** **Mitigation:** sentinel is a 1×1px `<div>` outside the SVG; observed against the scroll container.
- **R3 — SSE envelope and `getSessionEvents(after=…)` race.** Envelope arrives, page issues append-fetch with `after=newest`, fetch returns the just-broadcast row, page receives same event_id twice. **Mitigation:** `appendOne` dedupes by `event_id` (existing slice-8 SSE backfill pattern).
- **R4 — `rebuild_session` transaction lock blocks ingest.** WAL writers vs single-writer SQLite: transaction holds the write lock for the duration of compute. **Mitigation:** compute graph in memory first (no DB writes), then open transaction → DELETE + INSERT → commit. Lock window is only the actual write phase. Confirm via L2 timing test if needed.

---

## 10. Acceptance

slice-9 ships when **all** of:

1. `GET /v1/sessions/:id/events?before=...` returns older window with no overlap and no gap.
2. `GET /v1/sessions/:id` no longer returns `events`. All callers migrated.
3. `rebuild_session` atomic — L2 chaos test passes 20/20 parallel observations.
4. WebUI Timeline scrolls back ≥ 5 pages without reload, live append continues to work.
5. claude-in-chrome smoke (3 scenarios) green on bahamoth's 9000+ event session.
6. No more 1000ms debounce / setState identity-skip references; DEV-S8-13 removed from active code.
7. implementation-notes documents what survived from slice-8 and what was replaced.
