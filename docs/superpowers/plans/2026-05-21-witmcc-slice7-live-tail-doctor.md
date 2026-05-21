# Slice-7 Live Tail + Doctor v0.2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development or superpowers:executing-plans.
> **Commit messages:** Do NOT add `Co-Authored-By: Claude...` footers (pre-commit hook rejects them).

**Goal:** finish M1 — every source live without `witmcc ingest --all`; doctor reflects the actual Claude Code settings hierarchy + plugin hooks, not just `~/.claude/settings.json` and process env.

**Spec:** `docs/superpowers/specs/2026-05-21-witmcc-slice7-live-tail-doctor-design.md`

---

## File Structure

| Path | Action | Responsibility |
|---|---|---|
| `src/transcript_tail.rs` | create | `notify` watcher on transcripts root; byte-offset cursor; incremental line ingest. |
| `src/cli.rs` | modify | `Serve` gains `--transcripts-root <PATH>`, `--no-watch-transcripts`. `Doctor` gains `--project <PATH>`. |
| `src/main.rs` | modify | Spawn transcript_tail task inside `serve_cmd` alongside file/git watchers. |
| `src/lib.rs` | modify | `pub mod transcript_tail;` |
| `src/doctor.rs` | modify | Replace single-scope inspection with hierarchy walk + plugin manifests + scope attribution. |
| `src/doctor/settings.rs` | create (split) | `scopes()`, `effective_env()`, `hook_entries()`, `managed_policy()`. |
| `tests/transcript_tail.rs` | create | End-to-end tail tests against a tempdir transcripts root. |
| `tests/doctor.rs` | modify | Add tests for project scope, local override, plugin manifest discovery, managed policy. |
| `README.md` | modify | "One-command" path; doctor v0.2 sample output. |
| `docs/implementation-notes.html` | modify | slice-7 section. |

---

## Branching

Already on `slice7-live-tail-doctor` branched from `main` post slice-6 merge.

```bash
git status   # should be clean
git log --oneline main..HEAD   # initially empty
```

---

## Task 1: Commit design spec + plan

- [ ] Stage `docs/superpowers/specs/2026-05-21-witmcc-slice7-live-tail-doctor-design.md` + this plan.
- [ ] Commit: `docs(slice-7): design spec + TDD plan — transcript live tail + doctor v0.2`

---

## Task 2: Transcript tail module

**Files:** create `src/transcript_tail.rs`, modify `src/lib.rs`.

- [ ] **Step 1:** `pub async fn run(pool: SqlitePool, root: PathBuf, cancel: CancellationToken) -> anyhow::Result<()>`.
- [ ] **Step 2:** On startup, query `SELECT source_uri, COALESCE(MAX(source_line_no), -1) FROM raw_event WHERE source_type='claude_transcript' GROUP BY source_uri` and seed `HashMap<PathBuf, CursorState>` where `CursorState { byte_offset, next_line_no }`. To derive byte offset from line count, open each file once and count newlines while accumulating bytes; cache the result. (Bounded by total transcripts size; for typical users < 100 MB.)
- [ ] **Step 3:** Spawn `notify::recommended_watcher` on `root` recursive. Filter events to paths ending `.jsonl`. Coalesce events for the same path with a 100 ms debounce (mirror `src/watcher.rs`).
- [ ] **Step 4:** For each touched path:
  1. If cursor missing, treat as new file; seek to 0.
  2. Open file, `seek(SeekFrom::Start(cursor.byte_offset))`.
  3. `BufReader::read_line` loop until EOF; for each line: parse + `ingest::transcript::parse_line` + `ingest::store::write_line`-equivalent (reuse the slice-1 path; if it's not pub, expose `pub(crate)` helper).
  4. Update cursor `byte_offset` and `next_line_no` after successful write of each line.
  5. If file is shorter than cursor.byte_offset (truncation), reset cursor to 0 and start over (dedup absorbs repeats).
- [ ] **Step 5:** Cancellation: `select!` on the notify rx and `cancel.cancelled()`. Drain pending events on shutdown.
- [ ] **Step 6:** Log per-batch counts at `info` level; per-line errors at `warn`. Never panic.

**Commit:** `feat(transcript): live tail — notify watcher + byte-offset cursor + incremental ingest`

---

## Task 3: Wire tail into `serve`

**Files:** `src/cli.rs`, `src/main.rs`.

- [ ] **Step 1:** Add to `Serve` subcommand:
  ```rust
  #[arg(long)]
  no_watch_transcripts: bool,
  #[arg(long)]
  transcripts_root: Option<PathBuf>,
  ```
- [ ] **Step 2:** In `serve_cmd`, after the existing file/git spawn block:
  ```rust
  if !no_watch_transcripts {
      let root = transcripts_root.clone()
          .or_else(paths::default_transcripts_root);
      if let Some(root) = root {
          let pool_cl = pool.clone();
          let tok_cl = cancel.clone();
          background.spawn(async move {
              if let Err(e) = transcript_tail::run(pool_cl, root, tok_cl).await {
                  tracing::error!(error=?e, "transcript tail exited with error");
              }
          });
      } else {
          tracing::warn!("no transcripts root found; --transcripts-root to override");
      }
  }
  ```
- [ ] **Step 3:** Smoke locally: start serve, `touch ~/.claude/projects/test.jsonl`, append a fake JSONL line, watch for the row to appear in raw_event.

**Commit:** `feat(cli): serve --transcripts-root / --no-watch-transcripts; tail spawned by default`

---

## Task 4: Transcript tail integration tests

**Files:** create `tests/transcript_tail.rs`.

- [ ] **Test 1:** Spawn tail against tempdir root. Create a session jsonl, write one valid transcript line. Within 1s, expect `raw_event` rows for `claude_transcript` ≥ 1.
- [ ] **Test 2:** Append a second line, no other writes. Expect exactly +1 row.
- [ ] **Test 3:** Restart the tail (drop the JoinHandle, respawn). The cursor is re-derived from `raw_event`. Append a third line. Expect exactly +1 row.
- [ ] **Test 4:** Truncate the file to 0 bytes, then append a fresh line. Expect at most +1 row (dedup absorbs the fact that the first line bytes match what was already ingested before truncation).
- [ ] **Test 5:** Run `witmcc::ingest::run_ingest_all` on the same root mid-tail. No duplicate inserts (UNIQUE constraint).

Use the slice-1 fixture format. Reuse `tempfile::tempdir`, drive cancellation via `CancellationToken`.

**Commit:** `test(transcript): live tail — 5 end-to-end scenarios incl. restart and truncate`

---

## Task 5: Doctor — split into submodules

**Files:** restructure `src/doctor.rs` → `src/doctor/mod.rs` + `src/doctor/settings.rs`.

- [ ] **Step 1:** Move v0.1 code into `mod.rs`. No behaviour change yet — refactor pass.
- [ ] **Step 2:** Add stub `mod settings;` with empty `pub fn scopes() -> Vec<SettingsScope> { vec![] }` etc.
- [ ] **Step 3:** Ensure `cargo build` + `cargo test --test doctor` still green.

**Commit:** `refactor(doctor): split into doctor/{mod,settings}.rs for v0.2 expansion`

---

## Task 6: Doctor — settings hierarchy walk

**Files:** `src/doctor/settings.rs`.

- [ ] **Step 1:** Implement `managed_paths()` per OS (macOS/Linux/Windows), including `managed-settings.d/*.json` alphabetical merge.
- [ ] **Step 2:** Implement `user_path()` → `~/.claude/settings.json`.
- [ ] **Step 3:** Implement `project_paths(start: &Path)` walking up from `start` until finding `.claude/settings.json` or hitting filesystem root or `.git`. Returns project shared + local.
- [ ] **Step 4:** `pub fn scopes(opts: &DoctorOpts) -> Vec<SettingsScope>` returns ordered list (managed first → user → project shared → project local). Each entry: `{kind, path, present: bool, parsed: Option<serde_json::Value>}`.
- [ ] **Step 5:** `pub fn effective_env(scopes: &[SettingsScope]) -> BTreeMap<String, EnvSource>` applying precedence rules (managed > local > project > user).
- [ ] **Step 6:** Unit tests inline: synthesise tempdirs with known settings files; assert the walk + precedence.

**Commit:** `feat(doctor): settings hierarchy walk + effective_env (managed > local > project > user)`

---

## Task 7: Doctor — hook entries + plugin manifests + managed policy

**Files:** `src/doctor/settings.rs`.

- [ ] **Step 1:** `pub fn hook_entries(scopes: &[SettingsScope], plugins_root: &Path) -> Vec<HookEntry>`. Walk each scope's `hooks` block: for each event name, descend into `hooks[].command` strings and substring-match `hooks/v1/events`. Append `HookEntry { event, command, scope }`.
- [ ] **Step 2:** Extend with plugin walk: one-level-deep under `plugins_root` (default `~/.claude/plugins`); for each plugin dir, read `plugin.json` or `manifest.json`, parse `hooks` block same way, scope = `plugin:<dirname>`.
- [ ] **Step 3:** `pub fn managed_policy(scopes: &[SettingsScope]) -> ManagedPolicy` collecting `allow_managed_hooks_only`, `disable_all_hooks`, `allowed_http_hook_urls`. Only the *managed* scope contributes (per docs).
- [ ] **Step 4:** Tests: tempdir with project hooks + a fake plugin; assert both are reported with correct scopes.

**Commit:** `feat(doctor): hook_entries across all scopes + plugin manifests + managed_policy detection`

---

## Task 8: Doctor — pretty output + JSON schema bump

**Files:** `src/doctor/mod.rs`.

- [ ] **Step 1:** Rewrite `print_pretty` to use the new sections from §7.5 of the design spec (settings files probed, effective OTel env with scope column, file-effective vs shell-effective divergence, hook forwarding with scope, managed policy, plugin hooks, server source table — already from v0.1).
- [ ] **Step 2:** Extend `DoctorReport` struct with `settings_scopes`, `effective_env`, `hook_entries`, `managed_policy`, `plugin_hooks`. Keep old fields for backwards-compat (so existing tests pass; mark them as derived from the new sources).
- [ ] **Step 3:** `--json` emits the extended report; old keys remain.
- [ ] **Step 4:** Exit code rules unchanged.

**Commit:** `feat(doctor): v0.2 output — scope attribution, effective env, plugin hooks, managed policy`

---

## Task 9: Doctor v0.2 tests

**Files:** `tests/doctor.rs`.

- [ ] **Test 1:** Tempdir with `<tmp>/.claude/settings.json` containing OTel env + hooks. Run doctor with `--project <tmp>` against a live server. Output mentions `(project shared`. Effective env table contains the OTel keys.
- [ ] **Test 2:** Add `<tmp>/.claude/settings.local.json` overriding the endpoint. Doctor picks the local value; scope column says `(local`.
- [ ] **Test 3:** Create a plugin fixture under a fake plugins root with `plugin.json` whose hooks forward to `/hooks/v1/events`. Override the plugins root via env (we add a `WITMCC_DOCTOR_PLUGINS_ROOT` env for tests). Doctor's plugin hooks section lists it.
- [ ] **Test 4:** Empty everything + unreachable server → exit 1; new sections gracefully render "(none)".
- [ ] **Test 5:** Process env diverges from file scope env — output mentions divergence.

**Commit:** `test(doctor): v0.2 — project/local scope, plugin manifests, env divergence`

---

## Task 10: README + implementation-notes

**Files:** `README.md`, `docs/implementation-notes.html`.

- [ ] **Step 1:** README: replace the existing capture procedure with the new "one-command" flow:
  ```bash
  # 1. one-time wire (Claude Code env + one hook forwarder)
  vi ~/.claude/settings.json   # see snippet below

  # 2. forever after
  witmcc serve --auto-migrate   # transcripts/file/git tails + OTel/hook receivers all live
  witmcc doctor                 # verify
  ```
  Add the doctor v0.2 sample output.
- [ ] **Step 2:** implementation-notes: new `slice-7` section with overview + intentional deviations (cursor strategy, default-on tail, scope walk decisions, plugin substring match scope) + commit reference.

**Commit:** `docs(slice-7): README one-command flow + implementation-notes section`

---

## Final Verification

```bash
cargo test --all -- --include-ignored
cd webui && npm test

# Manual end-to-end:
rm -f .witmcc.sqlite
./target/release/witmcc init-db
./target/release/witmcc serve --auto-migrate   # ← only this is running
# new shell:
cd /any/repo && claude   # short interactive session, /exit
# back in serve shell, within 5s:
sqlite3 .witmcc.sqlite "SELECT source_type, COUNT(*) FROM raw_event GROUP BY source_type;"
# expect: claude_transcript ≥ 1, otel-metrics ≥ 1, otel-logs ≥ 1, (otel ≥ 1 if traces beta on), hook ≥ 1 if forward registered
./target/release/witmcc doctor
# expect: all sections green; settings scopes labelled; effective env from settings.json shown
```

**Definition of Done:**

- All 8 acceptance criteria from design spec §11 hold.
- One-command flow works without `witmcc ingest --all`.
- Doctor v0.2 shows scope attribution and surfaces plugin hooks.
- README + implementation-notes updated.

---

## Branch Merge

```bash
git checkout main
git merge --no-ff slice7-live-tail-doctor
git tag witmcc-slice-7
```
