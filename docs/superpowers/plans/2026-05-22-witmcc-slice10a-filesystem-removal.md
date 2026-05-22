# Slice-10a Implementation Plan — Filesystem-source Removal

**Spec:** `docs/superpowers/specs/2026-05-22-witmcc-slice10a-filesystem-removal-design.md`
**Branch:** `slice10a-filesystem-removal`
**Strategy:** TDD red-first per task. Each task starts with a failing test (or a failing compile, for module-removal cases). No task is "done" until its test is green AND prior tasks are still green. Commit topology mirrors spec §9.

---

## Phase 0 — Branch & baseline

| # | Task | Action | Verify |
|---|---|---|---|
| 0a | Create slice-10a branch from `main` | `git checkout main && git pull && git checkout -b slice10a-filesystem-removal` | `git status` clean, branch tip == main |
| 0b | Capture baseline test count | `cargo test 2>&1 \| tail -5` and `cd webui && npx vitest run 2>&1 \| tail -3` | Record numbers in scratch — will compare post-slice |
| 0c | Confirm git2 + notify present | `cargo tree -e normal 2>/dev/null \| rg -e '(^git2 v\|^notify v)'` | Both crates appear at root level |

No commit yet.

---

## Phase 1 — Red-locking fixtures + extractor tests (L1)

| # | Task | Test (red-first) | Impl |
|---|---|---|---|
| 1 | Freeze real structuredPatch fixtures | Create `tests/fixtures/transcripts/real/structured_patch_v01.jsonl` containing 3 lines: **(a)** single-hunk Edit, **(b)** Write (always emits `structuredPatch: []`), **(c)** multi-hunk Edit. All `userModified: false`. From `~/.claude/projects/-Users-bahamoth-projects-whats-in-my-cc/*.jsonl`. Selected and shown to user before freeze. **Notes:** (i) no real `userModified: true` line exists in 9 transcripts (228 ops, 0 matches) → synthetic JSON in task 7; (ii) Edit no-op replace doesn't exist in real data (a no-op replace errors before tool_result); (iii) MultiEdit unused. | n/a (fixture) |
| 2 | Invariant assertion | `tests/transcript_structured_patch.rs::structured_patch_invariant_holds_for_real_fixtures`: for each fixture line, deserialise; assert `toolUseResult` exists, `filePath` is absolute, `userModified` is bool, `structuredPatch` is array, each hunk has `oldStart/oldLines/newStart/newLines: u32` + `lines: [string]`. **Red until parser type defined.** | Define `TranscriptStructuredPatch` + `TranscriptHunk` structs in `src/ingest/diff_hunk.rs`. `#[derive(Deserialize)]`, no logic yet. |
| 3 | Extractor — Edit case | `tests/transcript_structured_patch.rs::extract_hunks_from_edit_fixture`: feed fixture (a) to `extract_diff_hunks(event_id, tool_use_id, session_id, value)` → returns `Vec<DiffHunkRecord>` length == fixture's `structuredPatch.length`. First hunk has `file_path`, `lines_added > 0`, `lines_removed > 0`, `line_range_after = Some((newStart, newStart+newLines-1))`, `change_type = "modified"`, `introduced_by_event_id` == passed event_id, `introduced_by_tool_use_id` == passed tool_use_id, `user_modified == false`. | `extract_diff_hunks(...)` body. Counts `+`/`-` prefixed lines for `lines_added/removed`. `change_type` from `oldLines == 0` (added) / `newLines == 0` (deleted) / else (modified). Reads parent `toolUseResult.userModified` into every emitted hunk. |
| 4 | Extractor — Write case (empty patch by design) | `tests/transcript_structured_patch.rs::extract_no_hunks_from_write_fixture`: feed fixture (b). Returns empty Vec (Write tool_result always has `structuredPatch == []`). No error. | Guard in fn (empty array → empty Vec). |
| 5 | Extractor — multi-hunk Edit | `tests/transcript_structured_patch.rs::extract_multiple_hunks_from_multi_hunk_edit_fixture`: feed fixture (c) (Edit with ≥2 hunks). Returns Vec of matching length; each hunk has correct `line_range_after`, `lines_added/removed`; all share same `introduced_by_event_id` + `introduced_by_tool_use_id`. | Loop over `structuredPatch[]`. |
| 6 | Extractor — non-edit tool ignored | `tests/transcript_structured_patch.rs::extract_no_hunks_for_bash_or_read`: synthesise minimal tool_result for Bash + Read (no `structuredPatch`). Returns empty Vec. | Graceful `None` deserialisation. |
| 7 | Extractor — user_modified flag round-trip | `tests/transcript_structured_patch.rs::user_modified_flag_round_trip`: synthesise inline JSON mirroring fixture (a)'s shape but with `userModified: true` → hunks all have `user_modified = true`. Feed real fixture (a) → `user_modified = false`. Synthesis-vs-real tradeoff noted inline in the test comment + DEV-S10A-07. | Already covered by task 3 impl; assertion-only. |
| 8 | `DiffHunkRecord` schema lock | `tests/transcript_structured_patch.rs::diff_hunk_record_fields`: assert struct contains exactly the new column set per spec §4 including `user_modified: bool`. **No** `introduced_by_commit_sha`. | `DiffHunkRecord` struct in `src/ingest/diff_hunk.rs`. |
| 9 | Cargo dep audit (red precursor) | `tests/cargo_dep_audit.rs::no_git2_or_notify_in_lock`: parse `Cargo.lock`, assert `git2` + `notify` not in `[[package]]`. **Red until Phase 7 strips them.** | Test file only. |

**Commit 1 (`test(slice-10a): red-locking tests for structuredPatch + module removal`)** — tasks 1-9. Tasks 3-7, 9 fail red (extractor body empty, deps still present). Commit anyway; Phases 2 + 7 turn them green.

> **Self-check before commit 1**: tests exist? yes (9). New invariants real-data anchored? yes (4 fixtures). UI change? no. Generalisation from one sample? no — 4 fixtures covering 4 polarities, scope explicitly limited to "transcripts currently in `~/.claude/projects/`".

---

## Phase 2 — Green-pass extractor

| # | Task | Verify | Impl |
|---|---|---|---|
| 10 | Implement extractor body | Tasks 3-7 turn green. | Flesh out `extract_diff_hunks` per task 3 description, including `user_modified` propagation from parent `toolUseResult`. |
| 11 | `lines_added/removed` accuracy | (covered by task 3) | Count by line prefix; ignore leading context line. |
| 12 | `patch_preview` truncation matches slice-5 PATCH_PREVIEW_MAX_BYTES constant | n/a — re-export constant from `src/ingest/diff_hunk.rs` so behaviour is bit-identical. | Move constant from old `file_git.rs` into the new module. |

**Commit 2 (`feat(ingest): transcript structuredPatch extractor → DiffHunkRecord`)** — tasks 10-12.

> **Self-check**: tests red→green in this commit. real-data anchoring (Phase 1 fixtures). No UI. Sample size: 4 fixtures, explicit in test names.

---

## Phase 3 — DB migration (in-place 0001..0004 edits)

Per spec §4 option C: edit existing migration files in place. Dev DB is disposable; sqlx hash checks will invalidate any existing local DB. Implementation-notes (DEV-S10A-05) records the required user action.

| # | Task | Test (red-first) | Impl |
|---|---|---|---|
| 13 | Schema-shape lock test | `tests/migration_schema.rs`: apply migrations to fresh in-memory DB, query `sqlite_master`. Assert `file_event` absent, `commit` absent, `diff_hunk` columns match spec §4 including `user_modified INTEGER NOT NULL DEFAULT 0`. **Red until task 14.** | Test file only. |
| 14 | In-place edits to `0001..0004` | Task 13 turns green. | Walk `migrations/0001..0004` in order. (a) Wherever `file_event`, `commit` tables are CREATEd — delete those statements. (b) Wherever `diff_hunk` is created — change columns to the new shape (drop `introduced_by_commit_sha`, add `introduced_by_event_id`, `introduced_by_tool_use_id`, `user_modified`). (c) Wherever `EventKind` CHECK constraint or `kind` column comment lists `file_event/commit/file_history_snapshot` — narrow it. |
| 15 | `repo_diff_hunk` API matches new schema | `tests/diff_hunk_repo.rs`: `repo_diff_hunk::insert(pool, &DiffHunkRecord { user_modified: true, ... })` succeeds; `repo_diff_hunk::list_session(pool, sid)` returns row with the bool flag. | `src/db/repo_diff_hunk.rs` — update SQL + bind params. |
| 16 | Document dev DB disposal | Add a short note to commit body: "any existing dev DB must be deleted before next `witmcc serve`. Path: see `src/paths.rs`." Will also be in `DEV-S10A-05`. | Commit body text. |

**Commit 3 (`feat(db): in-place migration edits — reshape diff_hunk, drop file_event/commit, narrow EventKind`)** — tasks 13-16.

> **Self-check**: schema-shape test red→green in this commit. Existing tests that referenced `file_event` / `commit` table types will fail compile here. Expected; deleted in commit 4. Note: this commit invalidates dev DB — listed in PR description and in DEV-S10A-05.

---

## Phase 4 — Strip ingest::file_git + EventKind variants + demote FileHistorySnapshot

| # | Task | Test (red-first) | Impl |
|---|---|---|---|
| 17 | Catalogue deletions | `rg 'file_git\|FILESYSTEM_SESSION_ID\|EventKind::FileEvent\|EventKind::Commit\|EventKind::FileHistorySnapshot\|store_file_event\|store_commit\|extract_commit_records' src/ tests/ \| wc -l` — record number. Goal: zero post-commit. | Survey. |
| 18 | Demote FileHistorySnapshot — red lock | `tests/file_history_snapshot_demoted.rs`: parse a fixture transcript line whose `type=="file-history-snapshot"`; assert resulting ObservedEvent has `kind == "session_state"`, `subkind == Some("file_history_snapshot")`, payload preserved. **Red until task 20.** | Test file. |
| 19 | Delete `src/ingest/file_git.rs` | `cargo build` fails on missing module. | `rm src/ingest/file_git.rs`. Remove `pub mod file_git;` from `src/ingest/mod.rs`. |
| 20 | Drop EventKind variants + rewrite `file_history` mapping | Compile errors in `src/db/repo_observed.rs` + `src/api/sse.rs` + `src/ingest/mapping.rs`. Task 18 turns green after this. | (a) Remove `FileEvent`, `Commit`, `FileHistorySnapshot` variants from `EventKind` in `src/model/observed.rs`. (b) Rewrite `mapping.rs:289-302 (file_history)` to use the `session_state(...)` pattern from `mapping.rs:280-286`: `e.kind = SessionState; e.subkind = Some("file_history_snapshot");`. (c) Remove `file_history_snapshot` match arms in `repo_observed::row_to_kind` and `sse::kind_from_str` (they will now be matched by the `session_state` arm). |
| 21 | Delete obsolete tests | `tests/file_event_*.rs`, `tests/file_git_*.rs`, `tests/git_poller_*.rs`, **any test asserting `kind == "file_history_snapshot"` as a top-level kind**. List discovered files in commit body. | `rm` the files. |
| 22 | Verify task 17 count → 0 | Re-run command from task 17. | If non-zero, find remaining refs and remove. |

**Commit 4 (`refactor(ingest): remove file_git module, drop FileEvent/Commit, demote FileHistorySnapshot`)** — tasks 17-22. `cargo build` may still error in Phase 5 because `main.rs` references the spawn functions; cleared in commit 5.

> **Self-check**: deletion-locking is via Phase 7 task 9 (`cargo_dep_audit`) for `notify`/`git2`. FileHistorySnapshot demotion is locked by task 18 in *this* commit. Mention this in the commit body so a reviewer can see the test mapping.

---

## Phase 5 — Strip watcher + git_poller modules + CLI/serve wiring

| # | Task | Test (red-first) | Impl |
|---|---|---|---|
| 23 | Identify CLI flags to remove | `rg 'watch_path\|watch-path\|git_poll\|git-poll\|no_watch\|no-watch\|no_git_poll\|no-git-poll' src/cli.rs src/main.rs` — list. | Audit. |
| 24 | Delete `src/watcher.rs`, `src/git_poller.rs` | `cargo build` fails on missing modules. | `rm src/watcher.rs src/git_poller.rs`. Remove `pub mod watcher;` / `pub mod git_poller;` from `src/lib.rs`. |
| 25 | Remove spawn calls + CLI flags | Compile errors in `src/main.rs` / `src/cli.rs`. | Remove the spawn arms in `tokio::join!`, the args struct fields, the clap derive lines. |
| 26 | Remove `live.rs` source_type variant if present | `rg '"file_git"\|source_type.*file_git' src/` → empty. | Edit `src/live.rs` enum + SSE serialisation. |
| 27 | `witmcc doctor` updates | `tests/doctor_*.rs` (if any) — drop any "filesystem watcher" / "git repo" probes. | `src/doctor/...` — remove related checks. |
| 28 | `cargo build --all-targets` green | n/a | (fallout fixes) |

**Commit 5 (`refactor(serve): remove src/watcher.rs + src/git_poller.rs and CLI wiring`)** — tasks 23-28. `cargo build` green.

> **Self-check**: deletion-locking is Phase 7 task 9 (cargo_dep_audit). Acceptable trade-off — durable.

---

## Phase 6 — Wire transcript ingest → diff_hunk write path

| # | Task | Test (red-first) | Impl |
|---|---|---|---|
| 29 | Ingest path calls extractor | `tests/transcript_ingest_diff_hunk.rs`: ingest a fixture transcript with N edits via `ingest::transcript::run_for_file`. After ingest, `repo_diff_hunk::list_session(pool, sid).len() == Σ structuredPatch.len()` across edits, and at least one row has `user_modified = 1`. | `src/ingest/mapping.rs` or `src/ingest/transcript.rs` — after writing the tool_result ObservedEvent, call `diff_hunk::extract_diff_hunks(...)` and `repo_diff_hunk::insert(...)` for each result. |
| 30 | Idempotency on re-ingest | Same test: ingest twice; row count stable. | `repo_diff_hunk::insert` is dedup-by-PK; PK derivation must be deterministic (use `Sha256(event_id + hunk_index)` or similar). |
| 31 | Subprocess E2E | `tests/diff_hunk_subprocess.rs`: spawn `witmcc serve --once` against a fixture transcript, ingest, query SQLite directly for `diff_hunk` count + `user_modified` distribution. | Subprocess harness from slice-9 reused. |
| 32 | Browser smoke | Manual: `witmcc serve` in a project with an active session, open WebUI, confirm session loads, no console errors, event counts plausible, file_history_snapshot events render under session_state colour. | (manual gate) |

**Commit 6 (`feat(ingest): transcript-driven diff_hunk write path`)** — tasks 29-31. Browser smoke task 32 happens here (before commit 7).

> **Self-check**: tests for new pipeline exist. real-data anchoring (transcript fixture). UI browser smoke done.

---

## Phase 7 — Strip Cargo deps + final audit

| # | Task | Test (red-first) | Impl |
|---|---|---|---|
| 33 | `Cargo.toml` removal | Task 9 (Phase 1) cargo_dep_audit turns green. | Remove `git2 = ...`, `notify = ...` lines from `[dependencies]`. Check `[dev-dependencies]` too. |
| 34 | Final grep audit | `rg 'FILESYSTEM_SESSION_ID\|file_git\|src/watcher\|src/git_poller\|notify::\|git2::\|EventKind::FileHistorySnapshot' src/ tests/ webui/src/` → empty. | Survey + targeted edits. |

**Commit 7 (`chore(deps): remove git2 + notify from Cargo.toml`)** — tasks 33-34.

> **Self-check**: locking test (task 9) turns green here. No UI.

---

## Phase 8 — Spec doc updates

| # | Task | Action | Verify |
|---|---|---|---|
| 35 | Update `docs/02_technical_architecture_spec.html` | Reframe File/Git Observer callouts per spec §7 (3 locations). Add verification semantics paragraph clarifying git SHA is not a marker. Use the text-extraction helper to read sections; edit inline preserving self-contained HTML. | Extracted text shows new framing; no leftover `notify` / `git2` / `file_git` / `commit SHA` strings. |
| 36 | Update `docs/03_data_model_spec.html` | `DiffHunk` example: drop `introduced_by_commit_sha`, add `introduced_by_event_id` + `introduced_by_tool_use_id` + `user_modified`. Drop `file_event` / `commit` raw event references if present. Add structuredPatch invariant pointer. Update EventKind list (no FileEvent/Commit/FileHistorySnapshot top-level). | Extracted text matches §7. |
| 37 | Update `docs/06_mvp_execution_plan.html` | Sources row: remove "git diff", add "Claude tool-result diff hunks". Out-of-MVP list: explicitly add "git history tracking" + "commit-SHA as verification marker". | Extracted text matches. |

**Commit 8 (`docs(spec): 02-architecture + 03-data-model + 06-mvp file/git → transcript lineage`)** — tasks 35-37.

> **Self-check**: doc-only. Real-data anchoring n/a (spec).

---

## Phase 9 — implementation-notes + CLAUDE.md status

| # | Task | Action | Verify |
|---|---|---|---|
| 38 | Add slice-10a section to `docs/implementation-notes.html` | Mirror slice-9's structure: Overview / **8 DEV entries** (S10A-01..08 per spec §7) / Commit Reference table (filled with commit 1-9 SHAs). Include explicit "delete your dev DB" call-out in DEV-S10A-05. | Extracted text shows the new section; commit SHAs match `git log --oneline`. |
| 39 | Bump CLAUDE.md Status block | "slice-1~9 완료" → "slice-1~10a 완료" with the new bullet for "filesystem-source removal + transcript-only file lineage". | Read CLAUDE.md Status section. |

**Commit 9 (`docs(slice-10a): implementation-notes + status sync`)** — tasks 38-39.

> **Self-check**: doc-only. Past commit SHAs referenced must exist (commits 1-8 land first).

---

## Phase 10 — PR & merge

| # | Task | Action | Verify |
|---|---|---|---|
| 40 | Push branch | `git push -u origin slice10a-filesystem-removal` | Branch on origin |
| 41 | Open PR | `gh pr create --title "refactor(slice-10a): remove notify watcher + git poller, transcript-only file lineage" --body "<heredoc>"`. Body must call out **"existing dev DB must be deleted"** under a "Migration notes" section. | PR URL returned |
| 42 | PR body shape | Summary / motivation / scope / commit list / test counts / browser smoke evidence / spec-doc diff highlights / dev DB invalidation warning | Reviewer can read end-to-end |

No commit — PR creation only.

---

## Acceptance summary (mirror of spec §11)

- All cargo + vitest tests green.
- `cargo tree | rg -e '(^git2 v|^notify v)'` empty.
- `find src/ -name 'watcher.rs' -o -name 'git_poller.rs' -o -name 'file_git.rs'` empty.
- `rg 'FILESYSTEM_SESSION_ID' src/ tests/` empty.
- `rg 'EventKind::FileHistorySnapshot|EventKind::FileEvent|EventKind::Commit' src/` empty.
- `rg 'file_git|file_event|file_history_snapshot' webui/src/` empty.
- `sqlite3` shows `diff_hunk` has `user_modified`, `introduced_by_event_id`, `introduced_by_tool_use_id`; no `introduced_by_commit_sha`.
- WebUI loads bahamoth's largest real session (50k+ events from prior smoke) without console errors, shows accurate counts, file_history_snapshot renders under session_state colour.
- Spec docs reflect transcript-only file lineage.
- implementation-notes slice-10a section present, all 8 DEV entries with SHAs.
- CLAUDE.md Status reflects "slice-1~10a 완료".

---

## Risks (mirror of spec §12)

- Forgotten test references → Phase 4 task 17 grep, Phase 7 task 34 grep.
- Hidden WebUI branch on removed kinds → Phase 6 task 32 browser smoke.
- structuredPatch shape drift → Phase 1 task 2 invariant test red-fails loudly.
- External-edit blind spot → out of product scope (DEV-S10A-01).
- sqlx migration hash mismatch on existing dev DB → DEV-S10A-05 + PR body call-out.
