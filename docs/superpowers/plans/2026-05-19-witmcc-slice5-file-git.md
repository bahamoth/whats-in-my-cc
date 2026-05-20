# Slice-5 File/Git Observer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **Commit messages:** Do **not** add `Co-Authored-By: Claude...` (or any other Claude attribution) footers. The repository's pre-commit hook rejects commits containing them.

**Goal:** Capture filesystem mutations and git commits as `RawEvent` + `ObservedEvent`, materialise `file_event` / `git_commit` / `diff_hunk` graph nodes, and introduce the `DiffHunk` side-table so future slices can build file lineage edges (AC-3) without further schema work.

**Architecture:** `witmcc serve --watch <path>` spawns two background tokio tasks alongside the HTTP server. A `notify` 7.x file watcher emits debounced `file_event` records; a `git2` poller (default 5 s) emits one `git_commit` + N `diff_hunk` records per new commit. Both write through `ingest::file_git::store_*` which mirrors the slice-4 self-heal pattern (touched-session rebuild even on dedup). All file/git events share a synthetic `session_id = "filesystem"`.

**Tech Stack:** Rust 1.88, axum 0.7, sqlx 0.8 (SQLite), tokio 1.40, notify 7.x, git2 0.19 (vendored-libgit2), tokio-util 0.7 (CancellationToken). Webui: React 18, TypeScript 5, vitest 2.

**Spec:** `docs/superpowers/specs/2026-05-19-witmcc-slice5-file-git-design.md`

---

## File Structure

| Path | Action | Responsibility |
|---|---|---|
| `Cargo.toml` | modify | Add `notify = "7"`, `git2 = { version = "0.19", features = ["vendored-libgit2"] }`, `tokio-util = { version = "0.7", features = ["rt"] }`. |
| `src/model/meta.rs` | modify | `SCHEMA_VERSION` 0.3.0 → 0.4.0; add `PARSER_VERSION_FILE_GIT`. |
| `src/model/observed.rs` | modify | `EventKind::FileEvent` / `GitCommit` / `DiffHunk` variants + `as_str` map. |
| `migrations/20260520120000_0003_diff_hunk.sql` | create | `diff_hunk` table + `file_lineage_idx`. |
| `src/db/repo_diff_hunk.rs` | create | `insert`, `list_session`, `count_by_session` helpers. |
| `src/db/mod.rs` | modify | Re-export `repo_diff_hunk`. |
| `src/ingest/file_git.rs` | create | Parse + store for `file_event` / `git_commit` + `diff_hunk` (single module — file & git share canonical-JSON + dedup helpers). |
| `src/ingest/mod.rs` | modify | Re-export `file_git`. |
| `src/graph/build.rs` | modify | Three new branches (`FileEvent`, `GitCommit`, `DiffHunk`) + filter out non-conversation kinds from `turn_order`. |
| `src/cli.rs` | modify | `Serve { watch: Option<PathBuf>, git_poll_secs: u64, shutdown_after_ms: Option<u64> }`. |
| `src/main.rs` | modify | Pass new flags into `cli::serve`. |
| `src/api/mod.rs` | modify | `serve` spawns file watcher + git poller tasks alongside axum, wires `CancellationToken` into `with_graceful_shutdown`. |
| `src/watcher.rs` | create | File watcher loop (`notify` + 250 ms debounce + cancellation). |
| `src/git_poller.rs` | create | Git poller loop (`git2` + tick interval + cancellation). |
| `tests/fixtures/file_git/` | create | Static fixtures only if helpful; most tests build runtime repos in tempdir. |
| `tests/file_git_ingest.rs` | create | Backend integration: store_commit + dedup + binary + graph + diff_hunk table + synthetic-session reservation. |
| `tests/file_watcher_loop.rs` | create | Watcher-loop test (`#[ignore]` by default for CI stability). |
| `tests/api.rs` | modify | Update `schema_version` assertion 0.3.0 → 0.4.0. |
| `tests/otel_ingest.rs` | modify | Update `schema_version` assertion 0.3.0 → 0.4.0 if present. |
| `tests/repo_observed.rs` | modify | Same — update pinned schema. |
| `webui/src/api/laneMapping.ts` | modify | Add `'Files'`; map `file_event`/`git_commit`/`diff_hunk` → `Files`. |
| `webui/src/api/__tests__/laneMapping.test.ts` | modify | `LANES.length === 8`; new kinds map to `Files`. |
| `webui/src/components/Timeline.tsx` | modify | Add `Files` to placeholders + `byLane` init. |
| `webui/src/components/__tests__/Timeline.test.tsx` | modify | Update existing 7-lane assertion to 8; add `file_event` marker regression. |
| `webui/src/components/SourcePanel.tsx` | modify | Branches for `file_event` / `git_commit` / `diff_hunk` record types. |
| `webui/src/components/__tests__/SourcePanel.test.tsx` | modify | Three new render tests. |
| `README.md` | modify | New "File/Git observer (slice-5)" section: `--watch` flag, smoke command, known limits. |
| `docs/implementation-notes.html` | modify | Slice-5 Overview / Intentional Deviations / Commit Reference; update `localnav`. |

---

## Branching

Work happens on `slice5-file-git` branched from `main` (post slice-4 merge). The design-spec commit already exists on this branch as `b91f89e`.

```bash
git checkout slice5-file-git
git log --oneline main..HEAD   # should show only the design spec commit
```

If the branch has not been created yet:
```bash
git checkout main && git pull --ff-only
git checkout -b slice5-file-git
git cherry-pick <design-spec-commit>   # 4db62bb from the stale branch, if needed
```

---

## Task 1: Bump `SCHEMA_VERSION`, add `EventKind` variants, register deps

**Files:**
- Modify: `src/model/meta.rs`
- Modify: `src/model/observed.rs`
- Modify: `Cargo.toml`
- Modify: `tests/api.rs`
- Modify: `tests/otel_ingest.rs` (if it pins schema version)
- Modify: `tests/repo_observed.rs` (if it pins schema version)

- [ ] **Step 1: Pin assertions to the new version**

Update every assertion that pins `schema_version == "0.3.0"` (or `meta.schema_version`) to `"0.4.0"`. Grep first:
```bash
grep -rn "0\\.3\\.0" tests/ src/ webui/src/
```

- [ ] **Step 2: Confirm cargo test fails**

```bash
cargo test --test api
```
Expected: FAIL on the schema_version assertion.

- [ ] **Step 3: Bump constants and add EventKind variants**

`src/model/meta.rs`:
```rust
pub const SCHEMA_VERSION: &str = "0.4.0";
pub const PARSER_VERSION_FILE_GIT: &str = "file_git@0.1.0";
```

`src/model/observed.rs` — extend `EventKind` enum:
```rust
EventKind::FileEvent,
EventKind::GitCommit,
EventKind::DiffHunk,
```
Add matching arms in `as_str`:
```rust
EventKind::FileEvent => "file_event",
EventKind::GitCommit => "git_commit",
EventKind::DiffHunk => "diff_hunk",
```

`Cargo.toml` — append under `[dependencies]`:
```toml
notify       = "7"
git2         = { version = "0.19", default-features = false, features = ["vendored-libgit2"] }
tokio-util   = { version = "0.7",  features = ["rt"] }
```

- [ ] **Step 4: Run cargo build + cargo test, confirm pass**

```bash
cargo build
cargo test --test api
```

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock src/model/meta.rs src/model/observed.rs tests/
git commit -m "chore(meta): bump SCHEMA_VERSION 0.3.0 -> 0.4.0; add file_git EventKinds + deps"
```

---

## Task 2: Migration 0003 — `diff_hunk` table + repo

**Files:**
- Create: `migrations/20260520120000_0003_diff_hunk.sql`
- Create: `src/db/repo_diff_hunk.rs`
- Modify: `src/db/mod.rs`

- [ ] **Step 1: Write failing repo test**

Add a unit test in `src/db/repo_diff_hunk.rs` that inserts a row and reads it back. Initially the module does not exist, so `mod repo_diff_hunk;` in `src/db/mod.rs` should fail compile.

Skeleton:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrate;
    use sqlx::sqlite::SqlitePoolOptions;

    #[tokio::test]
    async fn insert_then_list_session() {
        let pool = SqlitePoolOptions::new().connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        let row = NewDiffHunk {
            diff_hunk_id: "hunk_1".into(),
            schema_version: "0.4.0".into(),
            session_id: "filesystem".into(),
            file_path: "a.rs".into(),
            change_type: "modified".into(),
            line_start_after: Some(42),
            line_end_after: Some(57),
            introduced_by_node_id: Some("nd_g_1".into()),
            related_observed_event_id: Some("ev_h_1".into()),
        };
        insert(&pool, &row).await.unwrap();
        let out = list_session(&pool, "filesystem").await.unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].diff_hunk_id, "hunk_1");
    }
}
```

- [ ] **Step 2: Confirm compile/test failure**

```bash
cargo test repo_diff_hunk
```
Expected: FAIL — module missing.

- [ ] **Step 3: Implement migration + repo**

`migrations/20260520120000_0003_diff_hunk.sql`:
```sql
CREATE TABLE IF NOT EXISTS diff_hunk (
    diff_hunk_id              TEXT PRIMARY KEY,
    schema_version            TEXT NOT NULL,
    session_id                TEXT NOT NULL,
    file_path                 TEXT NOT NULL,
    change_type               TEXT NOT NULL,
    line_start_after          INTEGER,
    line_end_after            INTEGER,
    introduced_by_node_id     TEXT,
    related_observed_event_id TEXT,
    created_at                TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS file_lineage_idx
    ON diff_hunk(file_path, diff_hunk_id);
```

`src/db/repo_diff_hunk.rs` — `NewDiffHunk` struct, `insert`, `list_session`, `count_by_session`. Follow the style of `repo_observed.rs`.

`src/db/mod.rs` — add `pub mod repo_diff_hunk;`.

- [ ] **Step 4: cargo test pass**

```bash
cargo test repo_diff_hunk
```

- [ ] **Step 5: Commit**

```bash
git add migrations/20260520120000_0003_diff_hunk.sql src/db/repo_diff_hunk.rs src/db/mod.rs
git commit -m "feat(db): migration 0003 + repo_diff_hunk side-table (file_lineage_idx)"
```

---

## Task 3: file_git parser + canonical-JSON + payload shapes

**Files:**
- Create: `src/ingest/file_git.rs`
- Modify: `src/ingest/mod.rs`

- [ ] **Step 1: Write failing unit tests**

In `src/ingest/file_git.rs` add `#[cfg(test)] mod tests` covering:

```rust
// FileRecord round-trips through canonical_json (byte stable on reorder)
// CommitRecord serialisation produces the spec-defined "git" payload shape
// HunkRecord stores diff_hunk_id, line_range_after, lines_added/removed
// subkind mapping: created/modified/deleted/renamed → snake stays the same
```

- [ ] **Step 2: Confirm cargo test fail**

```bash
cargo test --lib ingest::file_git
```

- [ ] **Step 3: Implement parser layer**

Types (no IO yet — IO lives in store/watcher):

```rust
pub enum FileChange { Created, Modified, Deleted, Renamed }

pub struct FileRecord {
    pub session_id: String,             // always "filesystem"
    pub path: String,                   // absolute
    pub change_type: FileChange,
    pub old_path: Option<String>,
    pub size_bytes: Option<u64>,
    pub observed_at: DateTime<Utc>,
}

pub struct CommitRecord {
    pub session_id: String,             // "filesystem"
    pub repo: String,
    pub sha: String,
    pub parents: Vec<String>,
    pub author_name: String, pub author_email: String, pub author_time: DateTime<Utc>,
    pub committer_name: String, pub committer_email: String, pub committer_time: DateTime<Utc>,
    pub message: String,
    pub branch: Option<String>,
    pub files_changed: Vec<String>,
}

pub struct HunkRecord {
    pub diff_hunk_id: String,
    pub session_id: String,
    pub file_path: String,
    pub change_type: String,            // "added" | "modified" | "deleted" | "renamed"
    pub line_range_after: Option<(u32, u32)>,
    pub introduced_by_commit_sha: String,
    pub patch_preview: String,          // truncated to 4 KB
    pub lines_added: u32,
    pub lines_removed: u32,
}
```

Implement:
- `pub fn file_record_to_payload(r: &FileRecord) -> serde_json::Value` (matches spec §5.7 file_event shape)
- `pub fn commit_record_to_payload(r: &CommitRecord) -> serde_json::Value`
- `pub fn hunk_record_to_payload(r: &HunkRecord) -> serde_json::Value`
- `fn canonical_json(value: &Value) -> String` (lift from `ingest::hook::canonical_json`)
- `pub const FILESYSTEM_SESSION_ID: &str = "filesystem";`

`src/ingest/mod.rs`:
```rust
pub mod file_git;
```

- [ ] **Step 4: cargo test pass**

```bash
cargo test --lib ingest::file_git
```

- [ ] **Step 5: Commit**

```bash
git add src/ingest/file_git.rs src/ingest/mod.rs
git commit -m "feat(ingest): file_git records + payload shapes + canonical_json"
```

---

## Task 4: `store_file_event` — raw + observed + self-heal

**Files:**
- Modify: `src/ingest/file_git.rs`

- [ ] **Step 1: Write failing ingest test**

In `file_git.rs` tests:
```rust
#[tokio::test]
async fn store_file_event_persists_and_dedupes() {
    // build pool, migrate
    // record = FileRecord{ path:"/tmp/a.rs", change_type: Modified, ... }
    // store_file_event(&pool, record, now()) → IngestResult { accepted=1, duplicates=0, sessions=["filesystem"] }
    // store_file_event same record → IngestResult { accepted=0, duplicates=1, sessions=["filesystem"] }
    // list_session(pool, "filesystem") → 1 row with kind=FileEvent
}
```

- [ ] **Step 2: Confirm fail**

```bash
cargo test --lib ingest::file_git::store_file_event
```

- [ ] **Step 3: Implement**

```rust
pub async fn store_file_event(
    pool: &SqlitePool,
    record: FileRecord,
    received_at: DateTime<Utc>,
) -> Result<IngestResult>
```

Pattern mirrors `ingest::hook::store`:
- `source_uri = format!("file://{}", record.path)`
- `payload_sha = hex(Sha256(canonical_json(file_record_to_payload(&record))))`
- `repo_raw::insert_dedup` (source_type = `"file_git"`)
- `touched.insert(session_id)` BEFORE dedup-skip check (self-heal pattern)
- `ObservedEvent` with `kind = EventKind::FileEvent`, `subkind = Some(change_type as snake)`, `parser_version = PARSER_VERSION_FILE_GIT`
- `repo_observed::insert`
- `crate::graph::build::rebuild_session(pool, "filesystem")`

`IngestResult` struct: reuse same shape as hook's (rename to local `FileIngestResult` if separation helps).

- [ ] **Step 4: cargo test pass**

```bash
cargo test --lib ingest::file_git
```

- [ ] **Step 5: Commit**

```bash
git add src/ingest/file_git.rs
git commit -m "feat(ingest): store_file_event with raw dedup + self-heal rebuild"
```

---

## Task 5: `store_commit` — git_commit + diff_hunks

**Files:**
- Modify: `src/ingest/file_git.rs`

- [ ] **Step 1: Write failing tests**

```rust
#[tokio::test]
async fn store_commit_emits_commit_plus_hunks() {
    // build pool, migrate
    // build temp git repo via git2: write file, commit
    // run extract_commit_records(repo, head_oid) → (CommitRecord, Vec<HunkRecord>)
    // store_commit(&pool, commit_record, hunks, now()) → accepted_commits=1, accepted_hunks=N
    // observed_event rows: 1 git_commit + N diff_hunk
    // diff_hunk table rows: N with correct file_path
}

#[tokio::test]
async fn store_commit_dedupes_on_second_run() {
    // … same commit twice → duplicate_commits=1, duplicate_hunks=N on second
}

#[tokio::test]
async fn binary_file_diff_yields_null_line_range() {
    // commit a small PNG (or any non-utf8 blob) → hunk row has line_range_after = None
}
```

- [ ] **Step 2: Confirm fail**

```bash
cargo test --lib ingest::file_git::store_commit
```

- [ ] **Step 3: Implement**

Add to `file_git.rs`:

```rust
pub fn extract_commit_records(
    repo: &git2::Repository,
    commit: &git2::Commit,
) -> (CommitRecord, Vec<HunkRecord>)
```

Behaviour:
- Walk diff vs first parent (or empty tree if root commit), `git2::Diff::print(Patch, …)` with a callback that collects hunk metadata.
- `patch_preview` is the first ≤4 KB of the unified diff for that hunk; truncate with trailing `\n…[truncated]`.
- For binary diffs (`is_binary`), set `line_range_after = None`, `patch_preview = "<binary>"`, `lines_added = 0`, `lines_removed = 0`.
- `MAX_HUNKS_PER_COMMIT = 2000` — surplus dropped.
- `diff_hunk_id = format!("hunk_{}", ulid)` deterministic across re-runs? **No** — use ULID-from-monotonic-gen for the per-batch `MonotonicUlidGen`. Dedup happens at the raw layer via `payload_sha256`, so a fresh ulid on re-ingest is fine: the canonical payload sha matches and the raw row insert returns `inserted = false`. We then skip the diff_hunk INSERT inside the `if !inserted { continue; }` branch.

```rust
pub async fn store_commit(
    pool: &SqlitePool,
    commit_record: CommitRecord,
    hunks: Vec<HunkRecord>,
    received_at: DateTime<Utc>,
) -> Result<CommitIngestResult>
```

Steps inside:
- Insert `RawEvent` for the commit (`source_uri = "git://{repo}/commit/{sha}"`); persist `ObservedEvent` with kind = GitCommit, subkind = "commit".
- For each hunk:
  - Insert `RawEvent` (`source_uri = "git://{repo}/commit/{sha}/hunk/{file}:{a}-{b}"`).
  - Insert `ObservedEvent` (kind = DiffHunk).
  - Insert `diff_hunk` row pointing at the just-created observed_event_id.
- `touched.insert("filesystem")` BEFORE the first dedup-skip (self-heal).
- After loop: `rebuild_session(pool, "filesystem")`.

`CommitIngestResult`:
```rust
pub struct CommitIngestResult {
    pub accepted_commits: u64,
    pub duplicate_commits: u64,
    pub accepted_hunks: u64,
    pub duplicate_hunks: u64,
    pub dropped_hunks_over_limit: u64,
}
```

- [ ] **Step 4: cargo test pass**

```bash
cargo test --lib ingest::file_git
```

- [ ] **Step 5: Commit**

```bash
git add src/ingest/file_git.rs
git commit -m "feat(ingest): store_commit + extract_commit_records (git_commit + diff_hunks)"
```

---

## Task 6: Graph builder — file_event / git_commit / diff_hunk

**Files:**
- Modify: `src/graph/build.rs`

- [ ] **Step 1: Add failing branch test**

In `src/graph/build.rs` (existing `#[cfg(test)] mod tests` or new) add:
```rust
#[test]
fn file_event_node_keyed_by_path_and_change_type() { … }
#[test]
fn git_commit_node_keyed_by_sha() { … }
#[test]
fn diff_hunk_node_keyed_by_hunk_id() { … }
#[test]
fn turn_order_skips_file_git_kinds() { … }
```

- [ ] **Step 2: Confirm fail**

```bash
cargo test --lib graph::build
```

- [ ] **Step 3: Implement**

In `compute(...)`:

```rust
EventKind::FileEvent => (
    "file_event",
    json!({
        "session_id":  session_id,
        "file_path":   e.payload.pointer("/file/path"),
        "change_type": e.payload.pointer("/file/change_type"),
    }),
),
EventKind::GitCommit => (
    "git_commit",
    json!({
        "session_id": session_id,
        "sha":        e.payload.pointer("/git/sha"),
    }),
),
EventKind::DiffHunk => (
    "diff_hunk",
    json!({
        "session_id":   session_id,
        "diff_hunk_id": e.payload.pointer("/hunk/diff_hunk_id"),
    }),
),
```

Extend the `turn_order` filter:
```rust
.filter(|n| !matches!(n.node_kind.as_str(),
    "otel_span" | "file_event" | "git_commit" | "diff_hunk"))
```

- [ ] **Step 4: cargo test pass**

- [ ] **Step 5: Commit**

```bash
git add src/graph/build.rs
git commit -m "feat(graph): file_event / git_commit / diff_hunk nodes; exclude from turn_order"
```

---

## Task 7: CLI Serve flags — `--watch`, `--git-poll-secs`, `--shutdown-after-ms`

**Files:**
- Modify: `src/cli.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Write failing CLI test**

In `tests/cli_serve.rs` (create if missing):
```rust
#[test]
fn serve_accepts_watch_and_poll_flags() {
    use assert_cmd::Command;
    Command::cargo_bin("witmcc").unwrap()
        .args(["serve", "--bind", "127.0.0.1", "--port", "0",
               "--watch", "/tmp/nonexistent-witmcc-test",
               "--git-poll-secs", "1",
               "--shutdown-after-ms", "100"])
        .timeout(std::time::Duration::from_secs(5))
        .assert()
        .success();
}
```

- [ ] **Step 2: Confirm fail**

```bash
cargo test --test cli_serve
```

- [ ] **Step 3: Add CLI fields + plumb through**

`src/cli.rs`:
```rust
Serve {
    #[arg(long, default_value = "127.0.0.1")] bind: std::net::IpAddr,
    #[arg(long, default_value_t = 7878)]      port: u16,
    #[arg(long)]                              auto_migrate: bool,
    /// Watch a directory for file/git changes (slice-5).
    #[arg(long)]                              watch: Option<PathBuf>,
    /// Polling interval for new commits (seconds). Min 1.
    #[arg(long, default_value_t = 5)]         git_poll_secs: u64,
    /// (test-only) auto-shutdown after N ms.
    #[arg(long)]                              shutdown_after_ms: Option<u64>,
}
```

`src/main.rs` — pass fields into the serve entry point. Wire `watch` and `git_poll_secs` through; the watcher / poller tasks are stubs in Task 8/9 (a no-op shim for now is OK — just `if watch.is_some() { tracing::info!(?watch, "watch path provided"); }` for Task 7's pass).

- [ ] **Step 4: cargo test pass**

```bash
cargo build && cargo test --test cli_serve
```

- [ ] **Step 5: Commit**

```bash
git add src/cli.rs src/main.rs tests/cli_serve.rs
git commit -m "feat(cli): serve --watch / --git-poll-secs / --shutdown-after-ms"
```

---

## Task 8: File watcher task — `notify` + debounce + cancellation

**Files:**
- Create: `src/watcher.rs`
- Modify: `src/lib.rs` (add `pub mod watcher;`)
- Create or modify: `tests/file_watcher_loop.rs`

- [ ] **Step 1: Failing tokio test**

```rust
#[tokio::test]
#[ignore = "FS event timing flaky on CI"]
async fn watcher_emits_modified_event_within_1s() {
    // tempdir, spawn watcher task with cancellation token
    // write tempdir/a.txt
    // poll observed_event table — expect 1 row of kind=file_event subkind=created within 1500ms
    // cancel, await join
}
```

- [ ] **Step 2: Confirm fail**

- [ ] **Step 3: Implement**

```rust
// src/watcher.rs
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use tokio_util::sync::CancellationToken;

pub async fn run_file_watcher(
    pool: sqlx::SqlitePool,
    root: std::path::PathBuf,
    cancel: CancellationToken,
) -> anyhow::Result<()>
```

Behaviour:
- Spawn `notify::recommended_watcher` on `root` with `RecursiveMode::Recursive`.
- Forward events through a `tokio::sync::mpsc` channel (notify is sync).
- Debounce: a `HashMap<(PathBuf, FileChange), Instant>` window of 250 ms; flush per-key on tick.
- For each surviving event, build a `FileRecord` and call `crate::ingest::file_git::store_file_event`.
- Exclude paths matching `*.sqlite`, `*.sqlite-wal`, `*.sqlite-shm`, `.git/**`, `**/target/**`. Hardcoded glob list.
- Honour `cancel.cancelled()` to exit cleanly.

- [ ] **Step 4: cargo test pass (with `-- --ignored` for the watcher loop)**

```bash
cargo test --test file_watcher_loop -- --ignored
```

- [ ] **Step 5: Commit**

```bash
git add src/watcher.rs src/lib.rs tests/file_watcher_loop.rs
git commit -m "feat(watcher): notify-based file watcher with debounce + cancellation"
```

---

## Task 9: Git poller task — `git2` + interval + cancellation

**Files:**
- Create: `src/git_poller.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Failing integration test**

In `tests/file_git_ingest.rs` (created later in Task 11) or a small inline test:
```rust
#[tokio::test]
async fn poller_picks_up_new_commit() {
    // tempdir with git init + initial commit
    // spawn poller (1 s interval) + cancellation
    // make a 2nd commit
    // sleep 2 s
    // assert observed_event has 2 git_commit rows
    // cancel
}
```

Skip running this in CI by default — gate behind a feature flag or `#[ignore]`.

- [ ] **Step 2: Confirm fail**

- [ ] **Step 3: Implement**

```rust
// src/git_poller.rs
pub async fn run_git_poller(
    pool: sqlx::SqlitePool,
    repo_path: std::path::PathBuf,
    interval_secs: u64,
    cancel: CancellationToken,
) -> anyhow::Result<()>
```

Behaviour:
- Open `git2::Repository::open(&repo_path)` — log warn + early-return Ok(()) if missing.
- Track last-seen tip OID in memory; on startup, replay any commits since the last `git_commit` row in DB (use `repo_observed::list_session("filesystem")` filtered by `kind=git_commit` and the latest `payload.git.sha`).
- Tick `interval_secs` seconds; on each tick:
  - `repo.refname_to_id("HEAD")` — if new, walk `topo-order` from old tip (exclusive) to new tip (inclusive).
  - For each commit: `extract_commit_records` + `store_commit`.
- Honour cancellation between ticks.

- [ ] **Step 4: cargo test pass**

```bash
cargo test --test file_git_ingest -- --ignored
```

- [ ] **Step 5: Commit**

```bash
git add src/git_poller.rs src/lib.rs
git commit -m "feat(git): poller task — extract_commit_records on tick + cancellation"
```

---

## Task 10: Wire tasks into `cli::serve` + graceful shutdown

**Files:**
- Modify: `src/api/mod.rs` (or wherever `serve` lives)

- [ ] **Step 1: Failing smoke**

Extend `tests/cli_serve.rs` to assert that with `--watch` pointing at a tempdir containing `.git`, the process logs lines like `file watcher started` and `git poller started` to stderr. (We capture stderr via `assert_cmd` and grep.)

- [ ] **Step 2: Confirm fail**

- [ ] **Step 3: Implement**

In the serve entry point (find via `grep -n "fn serve" src/`):

```rust
let cancel = CancellationToken::new();
let mut handles = Vec::new();

if let Some(root) = watch.clone() {
    if root.exists() {
        let pool_cl = pool.clone();
        let tok = cancel.clone();
        handles.push(tokio::spawn(async move {
            if let Err(e) = crate::watcher::run_file_watcher(pool_cl, root, tok).await {
                tracing::error!(error=?e, "watcher exited");
            }
        }));
        let git_dir = root.join(".git");
        if git_dir.exists() {
            let pool_cl = pool.clone();
            let tok = cancel.clone();
            let secs = git_poll_secs.max(1);
            handles.push(tokio::spawn(async move {
                if let Err(e) = crate::git_poller::run_git_poller(pool_cl, root, secs, tok).await {
                    tracing::error!(error=?e, "git poller exited");
                }
            }));
        }
    } else {
        tracing::warn!(?root, "watch path does not exist; collectors disabled");
    }
}

let server = axum::serve(listener, app)
    .with_graceful_shutdown({
        let tok = cancel.clone();
        async move { tok.cancelled().await; }
    });

// On Ctrl-C: cancel.cancel(); let server finish; join handles.
```

For `--shutdown-after-ms`, spawn a `tokio::time::sleep` task that calls `cancel.cancel()` after the delay.

- [ ] **Step 4: cargo test + manual smoke**

```bash
cargo build
mkdir -p /tmp/witmcc-smoke && (cd /tmp/witmcc-smoke && git init && touch a.txt && git add . && git -c user.email=t@t -c user.name=t commit -m init)
RUST_LOG=info ./target/debug/witmcc serve --bind 127.0.0.1 --port 7878 --watch /tmp/witmcc-smoke --git-poll-secs 1 --shutdown-after-ms 3000 --auto-migrate
# expect log lines: file watcher started ... git poller started ...
```

- [ ] **Step 5: Commit**

```bash
git add src/api/mod.rs
git commit -m "feat(serve): spawn file watcher + git poller, wire CancellationToken into graceful shutdown"
```

---

## Task 11: End-to-end integration tests

**Files:**
- Create: `tests/file_git_ingest.rs`

- [ ] **Step 1: Write failing tests**

```rust
// uses tempfile + git2 to build a repo in-process
#[tokio::test] async fn commit_emits_git_commit_plus_hunks() { … }
#[tokio::test] async fn hunk_table_row_per_observed_event() { … }
#[tokio::test] async fn re_ingest_same_commit_is_no_op() { … }
#[tokio::test] async fn binary_file_diff_yields_null_line_range() { … }
#[tokio::test] async fn graph_for_filesystem_session_has_file_git_nodes() { … }
#[tokio::test] async fn filesystem_session_id_is_reserved() {
    // ingest a transcript with a session_id literally equal to "filesystem"
    // → expect either rejection or distinct table presence; doc the chosen behaviour
}
```

- [ ] **Step 2: Confirm fail**

- [ ] **Step 3: Make them pass**

Most logic is already in the store layer; tests just exercise it via temp git repos. Use `git2::Repository::init` + `commit_extended` to build commits.

- [ ] **Step 4: cargo test pass**

```bash
cargo test --test file_git_ingest
```

- [ ] **Step 5: Commit**

```bash
git add tests/file_git_ingest.rs
git commit -m "test(file_git): commit + hunks + dedup + binary + graph end-to-end"
```

---

## Task 12: UI — Files lane + SourcePanel sections + Timeline regression

**Files:**
- Modify: `webui/src/api/laneMapping.ts`
- Modify: `webui/src/api/__tests__/laneMapping.test.ts`
- Modify: `webui/src/components/Timeline.tsx`
- Modify: `webui/src/components/__tests__/Timeline.test.tsx`
- Modify: `webui/src/components/SourcePanel.tsx`
- Modify: `webui/src/components/__tests__/SourcePanel.test.tsx`

- [ ] **Step 1: Failing UI tests**

`laneMapping.test.ts`:
```ts
expect(LANES).toEqual(['Intent','Context','Action','State','Files','Hook','OTel','Quality']);
expect(laneForNodeKind('file_event')).toBe('Files');
expect(laneForNodeKind('git_commit')).toBe('Files');
expect(laneForNodeKind('diff_hunk')).toBe('Files');
```

`Timeline.test.tsx`:
- update existing "renders all seven lanes" to "renders all eight lanes" and add `'Files'` to the list.
- Add a new test that injects a `file_event` node and asserts the marker appears (mirrors the slice-3/4 regression).

`SourcePanel.test.tsx`:
- new test: `file_event` record renders `path` + `change_type` in the header.
- new test: `git_commit` record renders short sha + branch + message subject.
- new test: `diff_hunk` record renders `file_path` + line range + a `<pre>` patch_preview.

- [ ] **Step 2: Confirm fail**

```bash
just webui-test   # vitest run
```

- [ ] **Step 3: Implement**

`laneMapping.ts`:
```ts
export const LANES = ['Intent','Context','Action','State','Files','Hook','OTel','Quality'] as const;
…
case 'file_event': return 'Files';
case 'git_commit': return 'Files';
case 'diff_hunk':  return 'Files';
```

`Timeline.tsx` — add `Files: 'no file/git observations in this session'` to `PLACEHOLDERS` and `Files: []` to `byLane` init.

`SourcePanel.tsx` — three new branches conditioned on `state.data.record_type`:
- `'file_event'`: render `r.file.path` + change_type strip.
- `'git_commit'`: render `r.git.sha.slice(0,7)` + branch + first 80 chars of message; collapsible `<details>` for `files_changed`.
- `'diff_hunk'`: `r.hunk.file_path`, line range, `<pre>{r.hunk.patch_preview}</pre>`.

- [ ] **Step 4: vitest pass**

```bash
just webui-test
```

- [ ] **Step 5: Commit**

```bash
git add webui/src/api/laneMapping.ts webui/src/api/__tests__/laneMapping.test.ts \
        webui/src/components/Timeline.tsx webui/src/components/__tests__/Timeline.test.tsx \
        webui/src/components/SourcePanel.tsx webui/src/components/__tests__/SourcePanel.test.tsx
git commit -m "feat(webui): Files lane (8th); file_event/git_commit/diff_hunk markers + SourcePanel"
```

---

## Task 13: README + implementation-notes

**Files:**
- Modify: `README.md`
- Modify: `docs/implementation-notes.html`

- [ ] **Step 1: README section**

Append after the slice-4 section:

```md
### File/Git observer (slice-5)

`witmcc serve --watch <path>` spawns a filesystem watcher and (if `<path>/.git`
exists) a git poller. Both write to the synthetic session `"filesystem"`.

```bash
mkdir -p /tmp/witmcc-smoke && (cd /tmp/witmcc-smoke && git init)
./target/release/witmcc serve --bind 127.0.0.1 --port 7878 \
  --watch /tmp/witmcc-smoke --git-poll-secs 5 --auto-migrate
echo hi > /tmp/witmcc-smoke/a.txt
( cd /tmp/witmcc-smoke && git add . && git commit -m smoke )
sleep 7
curl http://127.0.0.1:7878/v1/sessions/filesystem/graph | jq '.data.nodes[].node_kind' | sort -u
# expect: diff_hunk, file_event, git_commit
```

Flags:
- `--watch <path>` — directory to observe; watcher + poller disabled if absent.
- `--git-poll-secs N` — poll interval (default 5). Min 1.

Known limits in slice-5:
- File mutations between commits surface as `file_event` only; no per-file
  content diff. Hunks come from `git commit` diffs only.
- Hunk text is truncated to 4 KB per hunk; binary diffs surface with
  `line_range_after = null` and `patch_preview = "<binary>"`.
- No redaction (M7). Hunks may carry secrets.
- `session_id="filesystem"` is reserved.
```

- [ ] **Step 2: `docs/implementation-notes.html`**

Add a `<section id="slice-5">` block with:
- Overview (one paragraph)
- Intentional Deviations: `DEV-S5-01`..`DEV-S5-NN` — each labelled with the spec section it deviates from (e.g., `DEV-S5-01: synthetic session id "filesystem"`).
- Commit Reference — list of slice-5 commits with one-line summaries.

Update the `<nav id="localnav">` to include the new section anchor.

- [ ] **Step 3: Final smoke**

```bash
just webui-build && cargo build --release
just webui-test && cargo test
```

- [ ] **Step 4: Commit**

```bash
git add README.md docs/implementation-notes.html
git commit -m "docs(slice-5): README file/git observer section + implementation notes"
```

---

## Final Verification

```bash
# Backend
just webui-build
cargo build --release
cargo test                                        # unit + ingest + repo
cargo test --test file_git_ingest                 # focused integration
cargo test --test file_watcher_loop -- --ignored  # local-only

# Frontend
just webui-test

# Smoke
mkdir -p /tmp/witmcc-smoke && (cd /tmp/witmcc-smoke && git init && touch a.txt && \
  git add . && git -c user.email=t@t -c user.name=t commit -m init)
rm -f /tmp/witmcc-smoke.db
./target/release/witmcc serve --bind 127.0.0.1 --port 7878 \
  --db-path /tmp/witmcc-smoke.db --watch /tmp/witmcc-smoke --git-poll-secs 1 \
  --auto-migrate &
sleep 2
echo hi >> /tmp/witmcc-smoke/a.txt
(cd /tmp/witmcc-smoke && git commit -am bump)
sleep 3
curl -sS http://127.0.0.1:7878/v1/sessions | jq '.data[] | select(.session_id=="filesystem")'
curl -sS http://127.0.0.1:7878/v1/sessions/filesystem/graph | jq '.data.nodes[] | .node_kind' | sort -u
# expect: diff_hunk, file_event, git_commit
kill %1
```

---

## Self-Review Checklist (maps to Acceptance Criteria in spec §12)

- [ ] **AC1** — `serve --watch <tempdir>` starts cleanly and logs both task startup lines → Task 10 smoke.
- [ ] **AC2** — file edit → `file_event` ObservedEvent within 1 s → Task 8 watcher test + Task 11 graph test.
- [ ] **AC3** — `git commit` → one `git_commit` + N `diff_hunk` ObservedEvents + N rows in `diff_hunk` → Task 11 `commit_emits_…` + `hunk_table_row_per_event`.
- [ ] **AC4** — `GET /v1/sessions/filesystem(/graph)` returns new kinds → Task 11 `graph_for_filesystem_session_…`.
- [ ] **AC5** — re-running ingest does not double-count → Task 4 dedup test + Task 11 `re_ingest_same_commit_is_no_op`.
- [ ] **AC6** — `SCHEMA_VERSION = 0.4.0` → Task 1.
- [ ] **AC7** — UI shows `Files` lane (8 total) + renders new node kinds → Task 12.
- [ ] **AC8** — all prior cargo + vitest tests still pass → Final Verification.
- [ ] **AC9** — README documents `--watch` + smoke + known limits → Task 13.
- [ ] **AC10** — `docs/implementation-notes.html` carries slice-5 deviations → Task 13.

---

## Risks specific to execution

| Risk | Mitigation |
|---|---|
| `git2` build failure on macOS arm64 | Use `vendored-libgit2` feature — verify `cargo build` works on the dev machine first commit. |
| Notify event flakiness on CI | All watcher-loop tests are `#[ignore]` by default. CI runs unit + integration only. |
| Synthetic `"filesystem"` collides with real session | Task 11 includes a regression test that documents the chosen policy. |
| `MAX_HUNKS_PER_COMMIT = 2000` truncation surprises | Task 5 logs a `parse_error` row when surplus is dropped; covered by a unit test. |
| Migration 0003 on existing 0.3.0 DBs | Migration is `CREATE TABLE IF NOT EXISTS`; no existing-row mutation; idempotent. |
