# Slice-5 Design — File/Git Observer + DiffHunk Schema

**Date:** 2026-05-19
**Branch:** `slice5-file-git` (stacked on `slice4-hook-collector`)
**Goal:** Capture local filesystem changes and git commit diffs as `RawEvent` + `ObservedEvent`, materialise `file_event` / `git_commit` / `diff_hunk` graph nodes, and introduce the `DiffHunk` schema so future slices can build file lineage (AC-3).

---

## 1. Motivation

PRD OBS-4 mandates observing "read/write/edit, diff hunk, pre/post snapshot, branch, commit, dirty state, verification command result" and requires that "특정 diff hunk에서 prompt, tool call, episode, verification run으로 역추적 가능해야 한다" (AC-3). Slices 1–4 capture **what Claude said and what tools were called**, but never **what changed on disk**. Without file/git state, every action chain ends at the tool call edge.

Slice-5 proves the data path: a running `witmcc serve --watch <path>` ingests filesystem events live and polls git for new commits, emitting nodes that future slices can wire into edges (`mutates`, `verifies`) without further schema work.

This slice **does not implement** the correlation edges, the `VerificationRun` auto-detection, or the file_lineage finding category — those become unlocked once the underlying nodes exist.

---

## 2. Scope

### In Scope

- New CLI flag `serve --watch <path>` that starts two background tokio tasks alongside the HTTP server:
  - **File watcher** (`notify` 7.x, cross-platform inotify/FSEvents/ReadDirectoryChangesW) emitting `file_event` ObservedEvents on create/modify/delete/rename.
  - **Git observer** (`git2` / libgit2 binding) polling for new commits and emitting `git_commit` ObservedEvent + one `diff_hunk` ObservedEvent per hunk in the commit's diff.
- New `RawEvent.source_type` value `"file_git"`.
- New `EventKind::FileEvent` and `EventKind::GitCommit` and `EventKind::DiffHunk` variants.
- New SQL table `diff_hunk` (separate from `observed_event` to preserve the entity in spec §07) with index `file_lineage_idx (file_path, diff_hunk_id)`. Migration 0003.
- Graph node materialisation for the three new kinds; **no new edge kinds in this slice** (lineage edges are a follow-up slice).
- UI: new `Files` lane carrying `file_event` + `git_commit` + `diff_hunk` markers; SourcePanel sections for each.
- Schema bump 0.3.0 → 0.4.0.
- Manual smoke documented in README: run `witmcc serve --watch ./test-repo`, edit a file, commit, verify nodes appear.

### Out of Scope (deferred to later slices)

- **`mutates` edges** (`tool_call` → `file_event`) — requires `(file_path, time-window)` correlation heuristic. Wired in a later "lineage" slice once observability of both sides is proven.
- **`verifies` edges** (`verification_run` → `diff_hunk`) — depends on `VerificationRun` auto-extraction.
- **`VerificationRun` schema** — defined in `docs/03_data_model_spec.html §07` but not materialised in slice-5. Add when test/build command detection is ready.
- **Uncommitted-edit diff hunks** — slice-5 captures hunks only from `git commit` diffs. Edits between commits surface as `file_event` records without `diff_hunk` (per tech-arch open question: "file snapshot 범위를 edit 전후 hunk 주변으로 제한할지, 전체 파일 snapshot을 허용할지" — we choose the lightweight branch).
- **Cross-source dedup** of file/git events vs transcript-implied edits.
- **Live transcript tail / hook tail / OTel push retries** — orthogonal.
- **Redaction** of file contents (M7). Hunks may carry secrets — README warns.
- **MCP server** (M6).
- **Findings on file state** (M5).

---

## 3. Architecture

```
                              ┌─────────────────────────┐
   serve --watch /path        │  tokio runtime          │
   ──────────────────────────▶│  ┌───────────────────┐  │
                              │  │ axum HTTP server  │  │
                              │  └───────────────────┘  │
                              │  ┌───────────────────┐  │
                              │  │ file watcher task │  │
                              │  │  (notify)         │  │
                              │  │   ↓ Event         │  │
                              │  │   ↓ debounce      │  │
                              │  │   ↓ ingest::file::│  │
                              │  │     store_event   │  │
                              │  └───────────────────┘  │
                              │  ┌───────────────────┐  │
                              │  │ git poller task   │  │
                              │  │  (git2, every Ns) │  │
                              │  │   ↓ new commits   │  │
                              │  │   ↓ commit + hunks│  │
                              │  │   ↓ ingest::git:: │  │
                              │  │     store_commit  │  │
                              │  └───────────────────┘  │
                              │  ┌───────────────────┐  │
                              │  │ shared SqlitePool │  │
                              │  └───────────────────┘  │
                              └─────────────────────────┘
```

Both background tasks are spawned by `cli::serve` *after* `auto_migrate` and *before* the axum bind. They share the same `SqlitePool` and rely on the existing slice-3 self-heal `rebuild_session` pattern after each batch.

If `--watch` is omitted, neither task starts: `serve` behaves identically to slices 1–4. Backward compatible.

---

## 4. CLI Surface

```
witmcc serve [--bind 127.0.0.1] [--port 7878] [--auto-migrate]
             [--watch <path>] [--git-poll-secs <N>]
```

- `--watch <path>`: absolute or relative path to a directory to observe. Required if any of the watcher tasks are to start. If `<path>/.git` exists, the git poller also activates against that repository; otherwise only the file watcher runs.
- `--git-poll-secs <N>`: polling interval for new commits. Default `5`. Minimum `1`. (Real-time hook-into-git is out of scope; polling is simple and sufficient for a single-user local tool.)

Session attribution: file/git events are NOT scoped to a specific Claude Code session automatically. Slice-5 emits them with `session_id = "filesystem"` (a synthetic well-known id). A later slice may infer per-Claude-session attribution via overlapping time windows + cwd matching from transcript/hook records. Until then, file/git data lives in its own pseudo-session row.

Rationale: spec is silent here; the synthetic-session approach keeps the data visible (`GET /v1/sessions` shows `filesystem`) without making slice-5 do correlation that AC-3 explicitly defers.

---

## 5. Data Model Changes

### 5.1 `EventKind` variants

Add three:

```rust
EventKind::FileEvent     // serialises as "file_event"
EventKind::GitCommit     // serialises as "git_commit"
EventKind::DiffHunk      // serialises as "diff_hunk"
```

### 5.2 `Actor`

Reuse existing variants. File/git events use `Actor::System`.

### 5.3 `parser_version`

New constant in `src/model/meta.rs`:

```rust
pub const PARSER_VERSION_FILE_GIT: &str = "file_git@0.1.0";
```

### 5.4 Schema version

Bump `SCHEMA_VERSION` from `0.3.0` to `0.4.0`. Existing 0.3.0 rows (and earlier) remain readable.

### 5.5 SQL migration 0003

`migrations/20260520120000_0003_diff_hunk.sql`:

```sql
-- Slice-5: DiffHunk side-table for file lineage queries.
-- Each row is also represented as an observed_event (kind=diff_hunk) so the
-- session-detail / graph endpoints surface it without join.  This side-table
-- exists to support spec-defined `file_lineage_idx` and future lineage queries
-- without scanning observed_event.payload JSON.

CREATE TABLE IF NOT EXISTS diff_hunk (
    diff_hunk_id          TEXT PRIMARY KEY,
    schema_version        TEXT NOT NULL,
    session_id            TEXT NOT NULL,
    file_path             TEXT NOT NULL,
    change_type           TEXT NOT NULL,   -- "added" | "modified" | "deleted" | "renamed"
    line_start_after      INTEGER,
    line_end_after        INTEGER,
    introduced_by_node_id TEXT,            -- node_id of the git_commit node
    related_observed_event_id TEXT,        -- FK observed_event.event_id (kind=diff_hunk)
    created_at            TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS file_lineage_idx
    ON diff_hunk(file_path, diff_hunk_id);
```

`related_episode_id` and `verification_refs` from the spec are **not** persisted in slice-5 (no episode or verification yet); the column is named `related_observed_event_id` to make the FK to `observed_event` explicit.

### 5.6 `RawEvent` for file/git

| Column | Value |
|---|---|
| `source_type` | `"file_git"` |
| `source_uri` | `file://<absolute path>` for file events; `git://<repo path>/commit/<sha>` for git_commit; `git://<repo path>/commit/<sha>/hunk/<file>:<line_start>-<line_end>` for diff_hunk |
| `source_line_no` | `0` |
| `source_byte_offset` | `0` |
| `payload_sha256` | sha256 of canonical JSON of the *normalised* event (not raw file content) |
| `payload` | canonical JSON of the normalised event |

Re-processing the same file event or the same commit yields an identical `payload_sha256` and dedupes via the existing UNIQUE constraint on `(source_uri, source_line_no, payload_sha256)`.

### 5.7 `ObservedEvent` payload shapes

**file_event**:
```jsonc
{
  "file": {
    "path":        "/abs/path/to/file.rs",
    "change_type": "modified",                 // created | modified | deleted | renamed
    "old_path":    "/abs/old/path",            // only on renamed
    "size_bytes":  4711,                       // best-effort post-event metadata
    "observed_at": "2026-05-19T09:12:00+09:00"
  }
}
```

**git_commit**:
```jsonc
{
  "git": {
    "repo":      "/abs/path/to/repo",
    "sha":       "abc1234...",
    "parents":   ["def5678..."],
    "author":    {"name": "x", "email": "y@z", "time": "2026-05-19T09:11:00+09:00"},
    "committer": {"name": "x", "email": "y@z", "time": "2026-05-19T09:11:00+09:00"},
    "message":   "fix: …",
    "branch":    "main",
    "files_changed": ["a.rs", "b.rs"]
  }
}
```

**diff_hunk** (also written to `diff_hunk` table):
```jsonc
{
  "hunk": {
    "diff_hunk_id":   "hunk_<ulid>",
    "file_path":      "a.rs",
    "change_type":    "modified",
    "line_range_after": {"start": 42, "end": 57},
    "introduced_by_commit_sha": "abc1234...",
    "patch_preview":  "@@ -40,3 +42,15 @@\n …",     // truncated to 4 KB
    "lines_added":    13,
    "lines_removed":  3
  }
}
```

### 5.8 Synthetic session

`session_id = "filesystem"` for all file/git events. `GET /v1/sessions` lists it like any other session. The session id is documented in README; users can later filter it out client-side. The choice is intentional: it makes file/git observability discoverable through the same endpoints as transcript/OTel without API surface changes.

---

## 6. Graph Mapping (`src/graph/build.rs`)

Add three branches:

```rust
EventKind::FileEvent => (
    "file_event",
    json!({
        "session_id": session_id,
        "file_path":  payload_str(&e, "file/path"),
        "change_type": payload_str(&e, "file/change_type"),
    }),
),
EventKind::GitCommit => (
    "git_commit",
    json!({"session_id": session_id, "sha": payload_str(&e, "git/sha")}),
),
EventKind::DiffHunk => (
    "diff_hunk",
    json!({"session_id": session_id, "diff_hunk_id": payload_str(&e, "hunk/diff_hunk_id")}),
),
```

No new edges in slice-5. The `turn_order` edge logic already filters out `otel_span`; extend it to also filter out `file_event`, `git_commit`, `diff_hunk` (they aren't conversation turns).

---

## 7. UI Changes

### 7.1 Lane mapping

Add `Files` lane between `State` and `Hook`. Updated `LANES`:

```ts
['Intent', 'Context', 'Action', 'State', 'Files', 'Hook', 'OTel', 'Quality']
```

`laneForNodeKind`:
```ts
case 'file_event':  return 'Files';
case 'git_commit':  return 'Files';
case 'diff_hunk':   return 'Files';
```

(8 lanes total. Timeline placeholder text added for `Files`: "no file/git observations in this session".)

### 7.2 SourcePanel

Three new branches in addition to the slice-3/4 ones:

- `record_type === 'file_event'`: header strip with `path` + `change_type`; full record JSON below.
- `record_type === 'git_commit'`: header strip with short sha + branch + message subject (first 80 chars); collapsible `files_changed` list; full record JSON below.
- `record_type === 'diff_hunk'`: header strip with `file_path` + line range; preformatted `patch_preview` block; full record JSON below.

### 7.3 Timeline

No structural change. `byLane` initialisation gains the new `Files: []` entry.

---

## 8. Error Handling & Edge Cases

| Case | Behaviour |
|---|---|
| `--watch <path>` does not exist | `serve` logs a warning and skips both watcher tasks; HTTP server still starts. |
| `<path>/.git` missing | File watcher runs; git poller silent. |
| File watcher emits a burst of events for the same file (e.g. editor save = unlink + create + chmod) | A 250 ms debounce coalesces them into one `file_event` per `(path, change_type)`. |
| Path leaves the watched directory (symlink, mount) | Events outside the watched root are dropped. |
| Git poll finds a non-FF advance (force push, history rewrite) | All commits between the previously seen tip and the new tip are emitted; the order is the natural `git log --topo-order` from old tip to new tip. |
| Commit with binary file diff | We emit one `diff_hunk` per binary file with `change_type=modified`, `line_range_after=null`, `patch_preview="<binary>"`. |
| Commit with 10,000+ hunks | Hunks are emitted batched; per-commit limit `MAX_HUNKS_PER_COMMIT = 2000`; surplus is dropped with a single `parse_error` row. |
| Hunk `patch_preview` exceeds 4 KB | Truncated with trailing `\n…[truncated]`. |
| Concurrent file events during git checkout | File watcher receives them as normal; git poller picks up the new tip on next tick. Both ingest independently; nothing requires synchronisation. |
| HTTP shutdown (Ctrl-C) | A `tokio_util::sync::CancellationToken` notifies both background tasks; they finish their in-flight batch and exit. |

---

## 9. Test Strategy

### 9.1 Unit tests

`src/ingest/file_git.rs` (new module, alongside `src/ingest/hook.rs`):

- `parse_file_event` produces a `FileRecord` for create/modify/delete/rename.
- `parse_git_commit` (with a `git2::Repository` over a tempdir): one commit → one `CommitRecord` with N hunks.
- `parse_git_commit` on an empty repo (HEAD missing) → empty result, no panic.
- Canonical JSON of FileRecord/CommitRecord/HunkRecord is byte-stable for re-emission.

### 9.2 Integration tests (`tests/file_git_ingest.rs`, new)

Uses `tempfile` + `git2` to build a small repo in test, then drives the ingest store directly (not via the watcher loop, which is harder to time in unit tests — the watcher loop has its own narrower test).

- `commit_emits_git_commit_plus_hunks`: init repo, write file, commit → store_commit → expect 1 `git_commit` row + N `diff_hunk` rows.
- `hunk_table_row_per_observed_event`: same commit → expect `diff_hunk` table has same N rows + `file_lineage_idx` is populated.
- `re_ingest_same_commit_is_no_op`: store same commit twice → second call yields `duplicate_commits=1, duplicate_hunks=N`.
- `binary_file_diff_yields_null_line_range`: commit a `.png` → hunk row has `line_start_after=null`.
- `graph_for_filesystem_session_has_file_git_nodes`: after ingest, `GET /v1/sessions/filesystem/graph` returns the new node kinds.

### 9.3 Watcher integration test (`tests/file_watcher_loop.rs`, new)

Uses a tempdir + a real `notify::recommended_watcher` wrapped in our debouncer:

- Write a file → observe a `file_event` row appears within 500 ms.
- Modify the same file rapidly 5 times → observe one debounced row (or at most 2 within 1 s).
- Delete the file → observe a `deleted` row.

This test is `#[ignore]` by default and tagged `@flaky_io` if FS event timing on CI proves unstable; manual smoke remains canonical.

### 9.4 CLI test

`tests/cli_serve.rs` (extend): `witmcc serve --watch <tempdir>` exits cleanly within 200 ms when given `--bind 127.0.0.1 --port 0 --shutdown-after-ms 100` (a new test-only flag; cleaner than killing the process).

### 9.5 UI tests

- `laneMapping.test.ts`: `file_event`/`git_commit`/`diff_hunk` map to `Files`; `LANES.length === 8`.
- `SourcePanel.test.tsx`: render hook-style tests for the three new record types.
- `Timeline.test.tsx`: 8 lanes; `file_event` node renders on `Files`.

### 9.6 Acceptance smoke

```bash
just webui-build && cargo build
mkdir -p /tmp/witmcc-smoke && cd /tmp/witmcc-smoke && git init && cd -
./target/debug/witmcc serve --bind 127.0.0.1 --port 7878 --watch /tmp/witmcc-smoke &
echo hi > /tmp/witmcc-smoke/a.txt
( cd /tmp/witmcc-smoke && git add . && git commit -m smoke )
sleep 7    # 5s git poll + slack
curl -sS http://127.0.0.1:7878/v1/sessions/filesystem | jq '.data.events[] | .kind' | sort -u
# expect: file_event, git_commit, diff_hunk
curl -sS http://127.0.0.1:7878/v1/sessions/filesystem/graph | jq '.data.nodes[] | .node_kind' | sort -u
```

---

## 10. Risks & Mitigations

| Risk | Mitigation |
|---|---|
| `notify` debouncer can drop events under heavy load | We use `notify::recommended_watcher` + a 250 ms tokio debounce buffer. Acceptable for slice-5; a future slice may move to `notify-debouncer-full` if loss is observed. |
| `git2` pulls in `libgit2` (C lib) — build issues on some platforms | Document MSRV / system dep in README. The `vendored-libgit2` feature can build it from source; we enable it by default. |
| Watcher fires events for our own SQLite WAL writes | We exclude paths matching `*.sqlite`, `*.sqlite-wal`, `*.sqlite-shm`, `.git/**`, `**/target/**` by default. Glob list is hardcoded in slice-5; configurable later. |
| Hunks with secrets get persisted | Redaction is M7. README warns. |
| Long-running tasks block graceful shutdown | All tasks listen to a single `CancellationToken`; HTTP server's `with_graceful_shutdown` uses the same. |
| Synthetic `session_id = "filesystem"` collides with real session id | The string `"filesystem"` is reserved; we document this and add a regression test that real sessions never get named `"filesystem"` in transcript/hook ingest. |

---

## 11. Migration Path

1. `cargo install`/`cargo build` after pulling slice-5 — auto-migration runs `0003_diff_hunk.sql`.
2. Existing rows continue working. `SCHEMA_VERSION` becomes `0.4.0`; older rows keep their stored version.
3. Users opt into watch by adding `--watch <path>` to their `serve` invocation. Default behaviour unchanged.

---

## 12. Acceptance Criteria for Slice-5

1. `serve --watch <tempdir>` starts cleanly when `<tempdir>` exists and contains a git repo; both background tasks log a startup line.
2. Editing a tracked file produces a `file_event` ObservedEvent with `subkind` ∈ `{created, modified, deleted, renamed}` within 1 s of the FS event.
3. `git commit` produces exactly one `git_commit` ObservedEvent and one `diff_hunk` ObservedEvent per hunk; each hunk also has a row in the `diff_hunk` table.
4. `GET /v1/sessions/filesystem` lists the new events; `GET /v1/sessions/filesystem/graph` returns nodes of kinds `file_event`, `git_commit`, `diff_hunk`.
5. Re-running the watcher against the same repo (after restart) does **not** double-count events: same commit → `payload_sha256` dedup; same file event → debounce + dedup combine.
6. SCHEMA_VERSION reports `0.4.0` via `/v1/sessions`.
7. UI Timeline shows the new `Files` lane (8 lanes total) and renders the three new node kinds.
8. All previously passing cargo + vitest tests continue to pass. New backend integration tests ≥5; new unit tests ≥4; new UI tests ≥3.
9. README documents `--watch` flag with smoke command and known limits.
10. `docs/implementation-notes.html` carries a slice-5 deviations section reflecting any decisions that diverged from this spec during implementation.

---

## 13. Open Decisions (resolved for this slice)

| Decision | Choice | Rationale |
|---|---|---|
| File watcher library | `notify` 7.x | Cross-platform, de-facto standard. Alternatives (kqueue direct, inotify direct) lock us to one OS. |
| Git ops library | `git2` (libgit2) with `vendored-libgit2` feature | Full feature parity, stable. `gix` (pure-rust) lacks some required APIs and adds complexity. |
| Watch trigger | File watcher + 5 s git poll | Real-time git hook integration is brittle and requires shell-side setup; polling is simple and the bound is small. |
| Hunk source | Git commit diffs only (no uncommitted edits) | Avoids implementing a content-diff engine; aligns with tech-arch "metadata-only if content unavailable" degrade. |
| `mutates` / `verifies` edges | Deferred to follow-up slice | Nodes-first is sufficient for AC-3 unblocking. Correlation logic requires its own decision space. |
| `VerificationRun` | Deferred (model not added) | Detection of test/build commands is a separate concern; spec puts it in the same section but acceptance criteria treats it independently. |
| Session attribution | Synthetic `session_id = "filesystem"` | Avoids per-session correlation logic in slice-5. Future slice can re-key file/git events into Claude sessions via overlapping time windows + cwd. |
| Migration policy | New `diff_hunk` table | Spec defines it as a first-class entity (§07). Keeps file_lineage queries fast without scanning JSON payload. |
| Schema bump | 0.4.0 | New variants + new table + new parser version. |
| CLI placement | `serve --watch <path>` (not a new subcommand) | Single long-running process. Keeps deployment story simple. |
| Default poll interval | 5 s | Balances commit latency vs git2 cost. Configurable via `--git-poll-secs`. |

---

## 14. Follow-up slices unblocked by this work

- **Lineage edges** (`mutates`, `verifies`) — `tool_call → file_event` and `verification_run → diff_hunk`.
- **Verification run extraction** — detect `cargo test`, `npm test`, etc. from tool calls and attach to file changes.
- **File lineage finding** — M5 finding category `missing_verification` requires both mutation nodes (this slice) and verification edges (next).
- **Session attribution heuristic** — re-key `session_id="filesystem"` events to overlapping Claude sessions based on time + cwd.
- **Live transcript tail** — orthogonal but symmetric; once file watching is solved, transcript tailing is a small generalisation.
