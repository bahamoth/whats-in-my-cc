# Slice-10a Design — Filesystem-source Removal · Transcript-only File Lineage

**Date:** 2026-05-22
**Branch:** `slice10a-filesystem-removal` (based on `main` post slice-9 PR #7 merge)
**Goal:** Remove the two non-session filesystem sources (`notify` watcher + `git2` poller) and the `FILESYSTEM_SESSION_ID` synthetic session. Re-anchor file lineage on transcript `toolUseResult.structuredPatch`, which is already session-scoped and structurally complete. M5 (Insight engine) is blocked on a uniform session-scoped data plane; this slice unblocks it.

---

## 1. Motivation

The slice-5 file/git source layer landed two collectors:

- `src/watcher.rs` — `notify`-based filesystem watcher emitting `FileRecord` (path, change_type, size_bytes).
- `src/git_poller.rs` — `git2`-based commit walker emitting `CommitRecord` + `HunkRecord` (sha, parents, patch hunks).

Both write to a synthetic `FILESYSTEM_SESSION_ID = "filesystem"` because neither has access to a real `sessionId` at the moment of capture. This breaks two invariants the rest of the codebase already enforces:

1. **Every ObservedEvent has a real session_id.** transcript / OTel / hook all carry one. Filesystem rows do not.
2. **Findings/lineage/episode resources are session-scoped.** M5 cannot say "this finding has evidence_refs in session X" if some evidence is on the `"filesystem"` pseudo-session.

The real-data investigation (9 transcripts, 200+ `Edit` tool calls, all carrying `toolUseResult.structuredPatch`) showed that transcript already captures, with **better fidelity**, every piece of information the two filesystem collectors emit — *for the events that fall in product scope* (Claude Code's own mutations). The filesystem-source comparison reduces to:

| Information | transcript `toolUseResult` | git poller | notify watcher | Decision basis |
|---|---|---|---|---|
| real session_id | ✅ line top | ❌ synthetic | ❌ synthetic | transcript wins |
| file path (absolute) | ✅ `filePath` | ✅ | ✅ `path` | transcript ≥ |
| structured diff hunks | ✅ `structuredPatch[]` (oldStart/oldLines/newStart/newLines/lines) | ✅ git2 patch | ❌ size_bytes only | transcript ≥ git, watcher loses |
| before/after content | ✅ `oldString`/`newString` | ⚠️ from patch only | ❌ | transcript ≥ |
| userModified flag | ✅ | ❌ | ❌ | transcript only |
| timing accuracy | ✅ at tool_result | ❌ at commit (delayed) | ✅ at fs event | transcript ≥ git on lineage timing |
| branch | ✅ `gitBranch` on every line | ✅ `branch` | ❌ | transcript covers |
| Claude-external edits | ❌ | ⚠️ (no session) | ⚠️ (no session) | out of product scope |
| structured commit (SHA/parents/author) | ⚠️ Bash result text only | ✅ structured | ❌ | git poller's only unique angle — but see §10 below |

Product boundary check (PRD §1): *"Claude Code 실행을 로컬에서 관측한다"*. Claude-external file edits and Claude-external commits are out of scope. The only structured commit case that overlaps scope (`Bash → git commit`) is itself a tool call captured by transcript; the SHA appears in the Bash `tool_result` stdout.

Net effect of keeping both collectors: **timing skew** (same edit observed twice with different timestamps) + **session-scope rule violation** + **noise** (watcher catches vim swap, build artefacts, etc.) without adding lineage information transcript does not already carry.

---

## 2. Scope

### In scope

- **Remove `src/watcher.rs`** entirely, plus the CLI flag / serve-runtime wiring that starts it.
- **Remove `src/git_poller.rs`** entirely, plus its CLI flag / serve-runtime wiring.
- **Remove `src/ingest/file_git.rs`** entirely. The record types (`FileRecord`, `CommitRecord`, `HunkRecord`) and helpers (`store_file_event`, `store_commit`, `extract_commit_records`, `commit_record_to_payload`, `hunk_record_to_payload`, `FILESYSTEM_SESSION_ID`) all go away. `EventKind::FileEvent` and `EventKind::Commit` variants removed from `src/model/observed.rs`. `source_type = "file_git"` retired from `raw_event`.
- **Demote `EventKind::FileHistorySnapshot` → `SessionState + subkind="file_history_snapshot"`**. Payload preserved verbatim. `mapping.rs:289-302` (the `file_history` fn) updated to use the existing `SessionState + subkind` pattern already used elsewhere in the file. Removes a now-disproportionate top-level variant in `EventKind` and tightens the enum after `FileEvent` / `Commit` are gone.
- **Remove `git2` + `notify` crates** from `Cargo.toml` (production deps; check dev-deps too).
- **Add transcript-side structuredPatch extraction**: When mapping `tool_result` ObservedEvents whose `toolUseResult` contains a `structuredPatch` array (Edit / Write / Update), emit a normalized `DiffHunk` record alongside the tool_result event, scoped to the real session.
  - Storage: `diff_hunk` table — schema reshaped (see §4). One row per hunk in the `structuredPatch[]` array.
  - Linkage: `introduced_by_event_id` = the tool_result event_id. `introduced_by_tool_use_id` = the matched tool_call's `tool_use_id`. No more `introduced_by_commit_sha`.
  - **Preserve `userModified` flag** as a `user_modified BOOLEAN NOT NULL` column on `DiffHunkRecord` / `diff_hunk` table. transcript's `toolUseResult.userModified == true` means a human hand-edited the file between Claude tool calls. We capture this signal at ingest time so M5 finding rules (`risky_action`) can read it without re-parsing payload. **Finding generation itself stays in M5** — slice-10a only locks the data.
- **Update spec docs**:
  - `docs/02_technical_architecture_spec.html` — "File/Git Observer" block + Stage 6 "file lineage" reference. Reframed to "File Lineage from Tool Results". Verification semantics paragraph updated to clarify that commit SHAs are not used as verification markers.
  - `docs/03_data_model_spec.html` — `DiffHunk` schema updated (drop `introduced_by_commit_sha`, add `introduced_by_event_id` + `introduced_by_tool_use_id` + `user_modified`). Remove `file_event` mention if present.
- **Update `docs/implementation-notes.html`** — new section for slice-10a documenting the removal, the structuredPatch invariant assertion approach, and commit reference.

### Out of scope (permanent — not deferred)

- **External-edit detection as a separate ObservedEvent stream** — we capture `userModified` on the hunk that ran into the modification, but we do not synthesise a "user modified file X at time T" event from polling the filesystem. Truly out-of-band edits (no surrounding Claude tool call) are by definition not observed by this product.
- **Commit-as-verification-marker** — recognising that a `Bash` tool_result stdout contains a git SHA-shaped string and treating it as a verification boundary. **Permanently out of product scope**: this is a backdoor reintroduction of git tracking, which we explicitly removed. M5 verification semantics will instead read from: (a) `Bash` test command results (`npm test`, `cargo test`, `pytest` stdout/exit), (b) hook PreToolUse / PostToolUse verification commands, (c) OTel verification spans when present. None of these depend on git.
- **`risky_action` finding generation from `userModified`** — slice-10a captures the flag; the finding rule that fires on it lives in M5.
- **Pull API surface changes** — no endpoints are added or removed. The events list endpoint already returned filesystem events only via the synthetic session, which no client (webui or test) opens. Sessions list will simply no longer include `"filesystem"` as a session_id row.

---

## 3. Real-data Invariant (structuredPatch)

`toolUseResult.structuredPatch` is undocumented in any public Claude Code reference we can cite. Per CLAUDE.md "Real-data anchoring", we lock its shape to a frozen real-fixture invariant rather than a docs URL.

### Invariant claim (locked by L1 test against fixture)

For every transcript `tool_result` line whose `message.content[*].tool_use_id` matches a preceding `tool_use` with `name ∈ {Edit, Write}`, the top-level `toolUseResult` object exists and contains an array `structuredPatch` where each element has:

- `oldStart: u32` — 1-based pre-image line number where the hunk starts. May be absent for full-file Write (in that case, `oldStart = 0`, `oldLines = 0` is the convention observed).
- `oldLines: u32` — count of pre-image lines in the hunk.
- `newStart: u32` — 1-based post-image line number.
- `newLines: u32` — count of post-image lines.
- `lines: [string]` — unified-diff lines, each prefixed with ` `, `-`, or `+`.

Plus sibling fields on `toolUseResult`: `filePath: string` (absolute), `oldString: string | null`, `newString: string | null`, `userModified: bool`, `replaceAll: bool` (for Edit).

### How we lock it

1. Add `tests/fixtures/transcripts/real/structured_patch_v01.jsonl` — frozen copies of three real `tool_result` lines: **(a)** `Edit` with single-hunk `structuredPatch`, **(b)** `Write` with `structuredPatch == []` (Write tools never emit hunks; the field is empty by design), **(c)** `Edit` with multi-hunk `structuredPatch` (≥2 hunks). All three have `userModified: false`.
2. Add `tests/transcript_structured_patch.rs` — L1 deserialise + invariant assertion test. Fails red until the parser + extractor are wired.
3. Add inline synthetic JSON for `userModified: true` polarity test — we verified 9 local transcripts (228 Edit+Write ops) contain **zero** `userModified: true` records. We cannot real-data-anchor this polarity. The polarity round-trip test uses hand-constructed JSON that mirrors fixture (a)'s shape but flips `userModified`. This is the only test in slice-10a that uses non-fixture JSON; recorded as a tradeoff in implementation-notes (DEV-S10A-07).

If a future Claude Code version changes this shape, the test goes red and we revisit. We do not encode a best-guess parser that silently degrades.

### Single-case vs. pattern

Counted across 9 local transcripts (228 Edit + Write ops):
- **Edit** tool: 199 calls, all with `structuredPatch.len() ≥ 1` (no empty-patch case observed — a no-op replace would error before reaching tool_result).
- **Write** tool: 29 calls, all with `structuredPatch == []`. Write's contract is full-file replace, not diff; the field is present-but-empty.
- **MultiEdit**: 0 calls. We deliberately do *not* extract from MultiEdit in slice-10a — there is no real data to lock the shape against.
- **`userModified: true`**: 0 records. Polarity test uses synthetic JSON only.

We sample 3 fixtures to assert the invariant holds across the observed polarities: single-hunk Edit, multi-hunk Edit, Write-with-empty-array. We do **not** claim every Claude Code version emits this — only that the versions producing the transcripts in `~/.claude/projects/` *do*, and that our parser fails loudly on shape drift.

---

## 4. Data Model Impact

### Tables

- `diff_hunk` — **schema reshaped** (see migration strategy below):
  - DROP `introduced_by_commit_sha TEXT`.
  - ADD `introduced_by_event_id TEXT NOT NULL`.
  - ADD `introduced_by_tool_use_id TEXT`. Nullable for the (currently zero) future case where we synthesise hunks without a tool_use_id.
  - ADD `user_modified INTEGER NOT NULL DEFAULT 0`. SQLite has no native BOOLEAN; we use INTEGER 0/1. Maps to Rust `bool`.
  - `session_id` stays NOT NULL — but the post-slice-10a invariant tightens its meaning: must equal a real transcript-derived session_id, never `"filesystem"`.
- `file_event` — **dropped** entirely (was populated only by `watcher.rs`).
- `commit` — **dropped** entirely if it exists as a standalone table (was populated only by `git_poller.rs`).
- `observed_event` — keep table. `kind = 'file_event' | 'commit'` rows will no longer be inserted; `kind = 'file_history_snapshot'` rows will be persisted as `kind = 'session_state' / subkind = 'file_history_snapshot'` instead (see §6 mapping change).

### Migration strategy — in-place edit of existing migrations (option C)

Per user decision: dev DB data is disposable. We modify the existing `migrations/0001..0004` files **in place** rather than adding a `0005_remove_filesystem_sources.sql`. Result: a fresh checkout looks like transcript-only was the design from the start; there is no "cleanup" migration in history.

Concretely:

1. `migrations/0001_*.sql` — if it created `EventKind` enum or `kind` constraint allowing `file_event` / `commit` / `file_history_snapshot`, narrow it.
2. `migrations/0002_*.sql` (or wherever `diff_hunk` was first defined) — change the column list to the new shape directly. No DROP COLUMN dance.
3. `migrations/0003_*.sql` (or wherever `file_event` / `commit` were first defined) — remove those CREATE TABLE statements.
4. `migrations/0004_*.sql` — adjust if it added any filesystem-related index.

Side effects:

- sqlx's migration hash validation will reject any existing dev DB (since the historical migration content no longer matches the recorded hash). Existing local DBs must be deleted (`rm ~/.witmcc/...` or wherever the DB lives — `src/paths.rs`).
- CI starts fresh on every run, so no impact there.
- `tests/` that create temp DBs via `sqlx::migrate!` will get the new schema directly.

`implementation-notes.html` records this as `DEV-S10A-05` with explicit user disposal instructions.

### ObservedEvent kinds

`EventKind` enum changes in `src/model/observed.rs`:

- REMOVE `FileEvent` (was filesystem watcher).
- REMOVE `Commit` (was git poller).
- **DEMOTE** `FileHistorySnapshot` → represented as `EventKind::SessionState` with `subkind = "file_history_snapshot"`. Payload unchanged. `src/ingest/mapping.rs:289-302` updated to use the existing `session_state(...)` helper pattern (see `mapping.rs:280-286` for the template).
- `repo_observed`'s string ↔ enum maps drop the three retired variants.

API consumers that branched on `"file_event"` / `"commit"` / `"file_history_snapshot"`:

- `src/api/sse.rs:222` (`file_history_snapshot` match arm) → remove arm; the row is now `session_state` and matches via that arm.
- `src/db/repo_observed.rs:376` (same match arm) → same fix.
- WebUI: `grep`'d — no component branches on these strings, so no UI change. The Timeline renders by `kind` colour but doesn't special-case file kinds.

---

## 5. API & WebUI Impact

### Pull API

- `GET /v1/sessions` — will no longer list `"filesystem"`. No code change needed; it just won't appear in the JOIN.
- `GET /v1/sessions/:id` for `id = "filesystem"` — returns 404 (was the only access path for filesystem events). No code change.
- `GET /v1/sessions/:id/events` for real sessions — unchanged surface. The events list previously did not include filesystem-sourced rows (they had a different session_id), so no client visibility change.
- `GET /v1/stream` (SSE) — `LiveEvent` enum drops `source_type = "file_git"` variant if it exists. Check `src/live.rs`.
- `GET /v1/events/:id/raw` — unchanged; will simply never receive an id from the dropped pipelines after this slice.

### WebUI

- No component currently renders `file_event` / `commit` events specially (verified via `grep -rn "file_event\|file_git" webui/src/`). No UI change required.

### CLI

- `witmcc serve` flags being removed: `--watch-path`, `--git-poll-interval`, `--no-watch`, `--no-git-poll` (exact names per slice-5; check current `src/cli.rs`).
- `witmcc doctor` checks: drop any "filesystem watcher running" / "git repo at $X" probes.

---

## 6. Code Removals (concrete inventory)

### Files deleted

- `src/watcher.rs` (full module, including its inline tests).
- `src/git_poller.rs` (full module).
- `src/ingest/file_git.rs` (full module — 900+ lines).
- `tests/file_watcher_*.rs`, `tests/git_poller_*.rs`, `tests/file_git_*.rs` (exact filenames TBD per current state).
- `tests/fixtures/git/*` if present.

### Files edited

- `src/cli.rs` — remove flags + their fields on the Args struct.
- `src/main.rs` — remove watcher/poller spawn calls and `tokio::join!` arms.
- `src/lib.rs` — remove `pub mod watcher`, `pub mod git_poller`. `pub mod ingest` keeps everything except its `file_git` sub-module.
- `src/ingest/mod.rs` — remove `pub mod file_git;`.
- `src/model/observed.rs` — drop `EventKind::FileEvent`, `EventKind::Commit`, **and `EventKind::FileHistorySnapshot`** variants and their string mappings. (FileHistorySnapshot becomes a `subkind` of `SessionState`.)
- `src/ingest/mapping.rs` — rewrite `file_history(...)` to use the `session_state(...)` pattern from `mapping.rs:280-286`. Sets `kind = SessionState`, `subkind = "file_history_snapshot"`, payload preserved.
- `src/db/repo_observed.rs` — drop matching match arms for `file_event`, `commit`, and `file_history_snapshot`. The latter is now stored as `session_state`.
- `src/db/repo_diff_hunk.rs` — repoint to transcript-sourced rows (column shape change per §4), add `user_modified` field handling.
- `src/live.rs` / `src/api/sse.rs` — drop `file_git` source_type handling. `file_history_snapshot` SSE envelope arm removed.
- `src/api/dto.rs` / `src/api/routes.rs` — no surface change but check for any `file_event` / `file_history_snapshot` rendering branches.
- `Cargo.toml` — remove `git2 = ...` and `notify = ...` (both production deps).
- `migrations/0001..0004` — in-place edits per §4 migration strategy (NO new migration file).

### Files added

- `src/ingest/diff_hunk.rs` (new module): given a transcript `tool_result` ObservedEvent and its `toolUseResult` JSON, extract `Vec<DiffHunkRecord>` (including `user_modified` from the parent `toolUseResult.userModified`) and persist via `repo_diff_hunk`.
- `tests/transcript_structured_patch.rs` — L1 invariant + extraction tests.
- `tests/fixtures/transcripts/real/structured_patch_v01.jsonl` — 3 real frozen lines.
- `tests/cargo_dep_audit.rs` — Cargo.lock-level lock test that `git2` and `notify` are absent.

### `Cargo.toml` dependency changes

- Remove: `git2`, `notify`.
- Verify no other crate depends on them transitively in a way that re-pulls them: a `cargo tree | grep -E '(git2|notify)'` after the change should return empty.

---

## 7. Spec Doc Updates

### `docs/02_technical_architecture_spec.html`

Three locations:

1. The Ingestion Layer diagram block currently lists:

   > File / Git
   > diff · state · verify

   Reframe to:

   > File Lineage
   > derived from transcript tool_results

2. The "File/Git Observer" callout currently reads:

   > File/Git Observer
   > file events, git diff/status, verification command
   > RawStateEvent + DiffHunk
   > degrade to metadata-only if content unavailable

   Replace with:

   > File Lineage from Tool Results
   > derives DiffHunk from transcript Edit / Write tool_result `structuredPatch`
   > no separate filesystem watcher or git poller
   > Claude-external edits and Claude-external commits are out of product scope

3. Stage 6 "Derive Products" — `file lineage` stays in the bullet list, source attribution updates to transcript.

4. **Verification semantics paragraph** (currently mentions "verification command" as part of File/Git Observer's output): rewrite to explicitly state that verification signals come from (a) `Bash` tool_result stdout/exit for known test commands, (b) hook PreToolUse/PostToolUse verification commands, (c) OTel verification spans. Git SHAs are NOT used as verification markers.

### `docs/03_data_model_spec.html`

1. `DiffHunk` JSON example: drop `verification_refs` (unimplemented anyway), update `introduced_by_node_id` semantics doc — it now points to the tool_call graph node. Add `introduced_by_event_id`, `introduced_by_tool_use_id`, and `user_modified` fields with their semantics. Remove `introduced_by_commit_sha`.
2. Drop any `file_event` / `commit` raw-event schema if present.
3. `EventKind` enum description: remove `file_event`, `commit`, `file_history_snapshot`. Note that `file_history_snapshot` data is preserved as `session_state` with `subkind = "file_history_snapshot"`.
4. Add a short subsection under "File Lineage and VerificationRun" that describes the structuredPatch invariant assumption (point to implementation-notes for the locked fixture).

### `docs/06_mvp_execution_plan.html`

1. Acceptance table AC-3 "File lineage": the *Pass condition* stays the same ("A diff hunk can be traced to tool call, episode, and verification run when observed"). The MVP scope row for Sources gets `git diff` removed and `Claude tool-result diff hunks` added.
2. Out-of-MVP / Out-of-scope list: explicitly add "git history tracking" and "commit-SHA as verification marker" to make the exclusion durable.

### `docs/implementation-notes.html`

New section `Overview (slice-10a)`, mirroring slice-9's structure: Overview / Intentional Deviations / Commit Reference. Key deviations to record:

- **DEV-S10A-01** — Removal of `src/watcher.rs` and rationale (transcript supersedes; `FILESYSTEM_SESSION_ID` synthetic session was a slice-5 design choice we now retire).
- **DEV-S10A-02** — Removal of `src/git_poller.rs` and rationale (no information transcript doesn't already have for in-scope events; timing skew was net negative).
- **DEV-S10A-03** — `structuredPatch` invariant locked by real-fixture test rather than docs URL (no public docs exist). Sample size: 3 fixtures from `~/.claude/projects/`.
- **DEV-S10A-04** — `EventKind::FileEvent` / `EventKind::Commit` removed. `EventKind::FileHistorySnapshot` **demoted to `SessionState` + subkind**; payload preserved.
- **DEV-S10A-05** — Migrations `0001..0004` edited in place rather than adding `0005`. Required user action: delete any existing dev DB. Sqlx hash check will fail otherwise.
- **DEV-S10A-06** — `git2` + `notify` removed from `Cargo.toml`. Build no longer requires libgit2.
- **DEV-S10A-07** — `userModified` flag preserved on `diff_hunk.user_modified` column. Finding generation (`risky_action`) explicitly deferred to M5; the column is signal-only in slice-10a.
- **DEV-S10A-08** — Commit-SHA-as-verification-marker permanently rejected as out-of-product-scope. Reasoning recorded so a future contributor doesn't reintroduce git tracking through this backdoor. M5 verification semantics will read from `Bash` test commands, hooks, and OTel spans.

---

## 8. Test Plan (TDD red-first)

### Red phase (before any implementation)

1. `tests/transcript_structured_patch.rs::extract_hunks_from_real_edit_fixture` — given the frozen `structured_patch_v01.jsonl` line for an `Edit`, parser returns 1 `DiffHunkRecord` with `file_path`, `lines_added`, `lines_removed`, `line_range_after`, `change_type = "modified"`, `introduced_by_event_id = <fixture event_id>`, `introduced_by_tool_use_id = <tool_use_id from input>`, `user_modified = <fixture's userModified bool>`. **Fails red until the new module exists.**
2. `tests/transcript_structured_patch.rs::extract_no_hunks_from_real_write_fixture` — `Write` tool_result has `structuredPatch == []` by design; extractor returns empty Vec. (Lineage for Write is carried by the surrounding `tool_call` ObservedEvent's payload — `file_path` lives there. DiffHunk is reserved for *modifications*.)
3. `tests/transcript_structured_patch.rs::no_hunks_for_empty_structured_patch` — fixture (c) (no-op replace) → empty Vec.
4. `tests/transcript_structured_patch.rs::no_hunks_for_non_edit_tool_results` — `Bash`, `Read`, `Grep` tool_results produce zero hunks.
5. `tests/transcript_structured_patch.rs::user_modified_flag_round_trip` — fixture with `userModified: true` produces hunks with `user_modified = true`. Fixture with `userModified: false` → `false`. (We pick one fixture of each polarity.)
6. `tests/cargo_dep_audit.rs::no_git2_or_notify_in_lock` — parse `Cargo.lock`, assert neither `git2` nor `notify` appears as a package. (Transitive presence allowed but not expected; spec assumption: `cargo tree` shows neither.)

### Green phase

7. Migrations `0001..0004` (in-place edited) apply cleanly to a fresh DB; `diff_hunk` schema matches §4 (including `user_modified` column). `file_event` and `commit` tables absent. Locked by `tests/migration_schema.rs`.
8. `repo_diff_hunk::insert` API updated to the new column shape; old callers (which are all in `file_git.rs`) compiled out.
9. Transcript ingest path now calls `ingest::diff_hunk::extract(...)` on every relevant `tool_result` and writes hunks. `file_history_snapshot` records map to `kind = session_state` with `subkind = "file_history_snapshot"`. Locked by `tests/file_history_snapshot_demoted.rs` (parses a fixture transcript line containing a `file-history-snapshot` record, asserts resulting ObservedEvent has `kind = session_state`).
10. `cargo build --all-targets` passes.
11. `cargo test` — all existing tests pass except those that depended on filesystem source / git poller (those are deleted in this slice).

### L3 subprocess

12. `tests/diff_hunk_subprocess.rs` — spawn `witmcc serve`, ingest one real transcript that contains ≥10 `Edit` tool calls, query `diff_hunk` via direct SQLite for that session, assert the count matches the sum of `structuredPatch.len()` per `Edit` line (Write rows expectedly produce zero hunks).

### Browser smoke

13. Open a real session in the WebUI. Confirm `MetaStrip` still shows accurate event counts, no console errors, no broken endpoint references (e.g. no stray `/v1/sessions/filesystem` from any client code). Verify `file_history_snapshot` events render under `session_state` colour in Timeline (no visual regression).

---

## 9. Migration Order (commit topology)

Each step is a single commit; CLAUDE.md self-check applies to each.

1. **`test(slice-10a): red-locking tests for structuredPatch + module removal`** — add §8.1-6 tests, all failing red. Add the 3 fixture lines.
2. **`feat(ingest): transcript structuredPatch extractor → DiffHunkRecord`** — add `src/ingest/diff_hunk.rs`; tests 1-5 turn green. `repo_diff_hunk` not touched yet; we just produce records in memory.
3. **`feat(db): in-place migration edits — reshape diff_hunk, drop file_event/commit, narrow EventKind`** — edit `migrations/0001..0004` in place per §4. Existing tests that reference `file_event` / `commit` tables go red here; we'll fix them by removing those tests in the next commit.
4. **`refactor(ingest): remove src/ingest/file_git.rs, EventKind::{FileEvent,Commit}; demote FileHistorySnapshot`** — remove the module, enum variants, and rewrite `mapping.rs:file_history` to use `session_state` pattern. Drop tests that referenced the dead pipeline. Test 9 (file_history_snapshot_demoted) turns green.
5. **`refactor(serve): remove src/watcher.rs + src/git_poller.rs and CLI wiring`** — remove modules and CLI flags.
6. **`refactor(ingest): wire transcript ingest → diff_hunk write path`** — `repo_diff_hunk::insert` (new column shape) called from `ingest::transcript` whenever the new extractor returns hunks. L3 subprocess test (§8.12) turns green.
7. **`chore(deps): remove git2 + notify from Cargo.toml`** — cargo dep audit test (§8.6) turns green.
8. **`docs(spec): 02-architecture + 03-data-model + 06-mvp file/git → transcript lineage`** — HTML spec edits.
9. **`docs(slice-10a): implementation-notes section + 8 deviations + commit reference`** — final implementation-notes update.
10. **`docs(claude-md): status — slice-10a 완료`** — bump CLAUDE.md Status block.

Browser smoke (§8.13) happens between commits 6 and 7 (after the wire-up, before the dep removal commit). If smoke fails, we fix in-place before commit 7.

---

## 10. Key Decisions / Open Questions

### Decided

- **Commit-as-verification-marker** — **permanently rejected** as out-of-product-scope (see §2 Out of scope and DEV-S10A-08). Reintroducing git-derived signals through this backdoor would undo the transcript-only design. M5 verification semantics will draw from `Bash` test commands, hooks, and OTel spans instead.
- **`userModified` handling** — captured as a `diff_hunk.user_modified BOOLEAN` column in slice-10a. Finding rule (`risky_action`) deferred to M5; this slice locks the data only.
- **`file-history-snapshot` records** — payload preserved verbatim, but stored as `kind = session_state / subkind = "file_history_snapshot"` rather than its own top-level `EventKind`. Maintains data continuity while tightening the enum.
- **Migration strategy** — option C: in-place edits to `migrations/0001..0004`. No `0005` cleanup migration. Dev DB disposal required (recorded in DEV-S10A-05).
- **Existing `diff_hunk` rows** — discarded with the dev DB. All current rows are from the `FILESYSTEM_SESSION_ID` pseudo-session; they have no real provenance to preserve and no UI consumes them.
- **`change_type` enum tightness** — keep freeform TEXT (matches slice-5 ergonomics). Tighten only when M3 episode segmentation requires filtering on it.
- **CLI escape hatch for filesystem watcher** — **no**. Local-first product; if a user wants out-of-band edit tracking, they can use a separate tool.

### Open

(none — all earlier open questions are now resolved.)

---

## 11. Acceptance

- All tests in §8 green.
- `cargo tree | rg -e '(^git2 v|^notify v)'` → empty.
- `find src/ -name 'watcher.rs' -o -name 'git_poller.rs' -o -name 'file_git.rs'` → empty.
- `rg 'FILESYSTEM_SESSION_ID' src/ tests/` → empty.
- `rg 'EventKind::FileHistorySnapshot|EventKind::FileEvent|EventKind::Commit' src/` → empty.
- `sqlite3 <dev-db> '.schema diff_hunk'` shows `user_modified`, `introduced_by_event_id`, `introduced_by_tool_use_id` columns and no `introduced_by_commit_sha`.
- `witmcc serve` starts and ingests a transcript; `GET /v1/sessions/:id/events` returns events with valid `session_id`s (none equal to `"filesystem"`).
- WebUI loads a real session, shows accurate event counts, no broken nodes, and `file_history_snapshot` events render under the `session_state` colour.
- Spec docs reflect the new architecture: no remaining mention of `notify`, `git2`, `filesystem watcher`, or `commit SHA as verification` as ingestion sources.
- `docs/implementation-notes.html` slice-10a section lists DEV-S10A-01..08 with commit SHAs.
- CLAUDE.md Status block reads "slice-1~10a 완료".

---

## 12. Risks

- **R-1:** A test we've forgotten depends on the synthetic filesystem session shape. *Mitigation:* `rg 'filesystem' tests/ src/` before commit 5; any remaining hits are addressed.
- **R-2:** WebUI silently broken because some component was branching on `kind === 'file_event'` or `kind === 'file_history_snapshot'` and now hits a dead branch. *Mitigation:* `rg 'file_event|file_git|file_history_snapshot|filesystem' webui/src/` before browser smoke.
- **R-3:** A future Claude Code version changes the `structuredPatch` shape, silently breaking the extractor. *Mitigation:* §3 invariant test fails red on shape drift; we don't degrade silently.
- **R-4:** External-edit blind spot — a truly out-of-band edit (no Claude tool call near it) is invisible. *Mitigation:* this is the cost of transcript-only; documented in PRD-aligned non-goals. `userModified` flag still catches the *next* Claude tool call's reaction to a stale state.
- **R-5:** sqlx migration hash mismatch on existing dev DB after in-place edits. *Mitigation:* spec §4 makes this explicit; DEV-S10A-05 calls out the user-action requirement (delete dev DB).
