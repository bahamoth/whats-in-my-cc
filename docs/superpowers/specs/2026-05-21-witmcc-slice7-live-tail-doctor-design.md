# Slice-7 Design — Transcript Live Tail + Doctor v0.2 (Multi-scope Settings)

**Date:** 2026-05-21
**Branch:** `slice7-live-tail-doctor` (based on `main` post slice-6)
**Goal:** Finish M1 — every Claude Code source (transcript / OTel / hook / file·git) lands in witmcc as it happens, with one `witmcc serve`. Doctor walks the actual Claude Code settings hierarchy + plugin manifests so its diagnostic reflects what `claude` will really see, not just what the current shell exports.

---

## 1. Motivation

Two gaps surfaced after slice-6 merge:

1. **Transcript was never live.** Slice-1 shipped `witmcc ingest --all` as a one-shot scan of `~/.claude/projects/**/*.jsonl`. The user has to remember to run it again after each `claude` session. M1's intent was "one running process gives a real-time view"; one signal still violates that.

2. **Doctor v0.1 saw the wrong thing.** It only reads `~/.claude/settings.json` and only inspects process env, so it produced false negatives on a system where settings were correctly wired but via project-scope or via the `env` block (which `claude` injects into its own child process but doesn't export back to the parent shell). Claude Code actually merges four settings scopes (managed → user → project → local) plus drop-in `managed-settings.d/`, and hook entries can also come from plugin manifests under `~/.claude/plugins/`.

Slice-7 closes both. After this slice:
- `witmcc serve` alone (no second invocation) gives a real-time view of every source as soon as the user wires hooks once.
- `witmcc doctor` reflects the *effective* Claude Code configuration and tells the user **which scope** each value came from.

---

## 2. Scope

### In Scope

- **Transcript live tail** — `notify` watcher on `~/.claude/projects/**/*.jsonl` (or `--transcripts-root <path>` override). New JSONL lines are incrementally ingested via the existing slice-1 parser/store. Cursor is in-memory `HashMap<source_uri, last_line_no>` seeded from `raw_event` on startup; idempotency falls back to the existing `(source_uri, source_line_no, payload_sha256)` unique. Spawned inside `serve` alongside the file/git watchers (slice-5 pattern).
- **CLI flags** — `--no-watch-transcripts` to disable; `--transcripts-root <PATH>` to override default. Default = on with autodetected root.
- **Doctor v0.2** — walks the full Claude Code settings hierarchy:
  - Managed (`/Library/Application Support/ClaudeCode/managed-settings.json` + `managed-settings.d/*.json` alphabetical merge, plus Linux `/etc/claude-code/` and Windows equivalents)
  - User (`~/.claude/settings.json`)
  - Project shared (walk up from `--project` or CWD to find `.claude/settings.json`)
  - Project local (`.claude/settings.local.json` next to project shared)
- **Effective env** = merge of all-scope `env` blocks (precedence: managed > local > project > user) ∪ process env (which takes precedence over file scopes for the running shell — but `claude` only sees the file scopes plus its own inheritable env). Doctor reports both *file-effective* (what `claude` would see when launched) and *shell-effective* (what the current shell has).
- **Hook entry discovery** — all scopes' `hooks` blocks ∪ plugin manifests under `~/.claude/plugins/**/{plugin.json,manifest.json,hooks.json}` (substring match for now; full plugin manifest schema is its own slice).
- **Managed policy detection** — `allowManagedHooksOnly`, `disableAllHooks`, `allowedHttpHookUrls`. If active, doctor warns that user/project hooks may be silenced even when present.
- **Scope attribution in output** — every reported value carries `(source = "project shared .claude/settings.json")` etc.
- **Implementation-notes + README updates** anchored on real outputs.

### Out of Scope (deferred)

- Plugin manifest **schema validation** — substring `"hooks/v1/events"` match is enough for diagnostic purposes; richer plugin awareness is its own slice.
- `claude` auto-launch wrapper / hook auto-install — CLAUDE.md non-goal stays inviolate. Doctor outputs copy-pastable snippets only.
- Transcript line-level **redaction** (M7).
- Cross-source dedup (otel-logs ↔ transcript ↔ hook) — separate slice, mentioned in slice-6 DEV-S6-06.
- Findings engine (M5).
- WebUI changes — neither feature surfaces new node kinds.

---

## 3. Architecture

```
                ┌──────────────────────────────────────────────────────┐
                │ witmcc serve (single binary, single process)         │
                │                                                       │
                │   axum router + tower layers                          │
                │     · /v1/health, /v1/sessions, /v1/events/:id/raw   │
                │     · /v1/health/sources                              │
                │     · /otel/v1/{traces,metrics,logs}                  │
                │     · /hooks/v1/events                                │
                │                                                       │
                │   background tokio tasks (cancellable):               │
                │     · file watcher    (--watch <repo>, slice-5)      │
                │     · git poller      (--watch <repo>, slice-5)      │
                │     · transcript tail (--transcripts-root, NEW)      │
                └──────────────────────────────────────────────────────┘
                            ▲                       ▲
                            │ POST                  │ JSONL append
                            │                       │
            ┌───────────────┴───────────┐  ┌────────┴───────────────┐
            │ claude code               │  │ ~/.claude/projects/    │
            │  · OTel metrics/logs/     │  │   <project>/<sid>.jsonl│
            │    traces  (settings env) │  │ (claude writes these)  │
            │  · hooks (settings hooks) │  └────────────────────────┘
            └────────────────────────────┘
```

The transcript tail mirrors slice-5's file watcher: `notify::RecommendedWatcher` on the transcripts root, debounced 100 ms (claude flushes faster than file/git), then for each touched `.jsonl` re-read from the cursor and feed lines through `ingest::store::write_line` (existing slice-1 path).

Doctor v0.2 is purely additive — same CLI, expanded internals, new fields in the JSON output. Existing `--json` consumers see more keys; existing pretty output gets a "scope" column.

---

## 4. CLI Surface

### `witmcc serve` (extended)

```
witmcc serve [...]
  --transcripts-root <PATH>     # default: ~/.claude/projects
  --no-watch-transcripts        # disable the tail (backfill via `ingest --all` still works)
```

`--watch <repo>` (slice-5) continues to control file/git watchers independently. The transcripts tail does *not* require `--watch`.

### `witmcc ingest --all` (unchanged)

Continues to work as **backfill**. Use cases:
- First-time setup before running serve.
- After a long downtime — picks up everything we missed.
- Reprocessing after a witmcc upgrade.

The tail uses the same parser, so re-ingesting is idempotent.

### `witmcc doctor` (extended)

Same flags (`--json`, `--server`). New optional:

```
  --project <PATH>     # treat <PATH> as the project root for scope walk; default = CWD
```

Output gains:
- A "scope" column per env value and per hook entry.
- A new "settings file" section listing every file probed with present/missing.
- A "managed policy" section if `allowManagedHooksOnly` / `disableAllHooks` is set.

---

## 5. Data Model

**No DB changes.** Both features are receiver/runtime concerns.

`raw_event` already carries `(source_uri, source_line_no, payload_sha256)` uniqueness; the tail's per-line incremental write naturally dedups with `ingest --all` or with itself across restarts.

`SCHEMA_VERSION` stays at `0.5.0`.

---

## 6. Transcript Tail Implementation

### 6.1 Module — `src/transcript_tail.rs` (new)

```rust
pub async fn run_transcript_tail(
    pool: SqlitePool,
    root: PathBuf,
    cancel: CancellationToken,
) -> anyhow::Result<()>;
```

Mirrors `src/watcher.rs::run_file_watcher` in shape:
1. Spawn `notify::RecommendedWatcher` on `root` recursive.
2. Maintain `HashMap<PathBuf, u64 /* next_line_no_to_read */>`.
3. On startup: query `SELECT source_uri, MAX(source_line_no)+1 FROM raw_event WHERE source_type='claude_transcript' GROUP BY source_uri` and seed the map.
4. On notify event for path ending `.jsonl`: open file, seek by line count (or maintain a byte-offset map for speed — choose byte offsets to avoid O(n) re-scan), read new lines, push through `ingest::store::write_line` (or `transcript::parse_line` + `repo_raw::insert_dedup`).
5. 100 ms in-memory debounce per path (mirror slice-5 watcher).
6. On error: log and continue (fail-soft per PRD OBS-3).

Decision: track **byte offsets** rather than line counts because some sessions write 100 MB JSONLs and re-counting lines on every flush is O(n²). Map: `HashMap<PathBuf, u64 /* byte_offset_of_next_unread */>`. Cursor recovery on startup: read each known file once, count newlines to derive line count, derive byte offset from the line count via incremental seek. This is a one-time O(total bytes) on cold start; the alternative — adding a `cursor` table — is overkill for the MVP.

### 6.2 Wiring — `src/main.rs::serve_cmd`

Spawn alongside file/git tasks when `transcripts_root` is `Some` and `!no_watch_transcripts`:

```rust
if let Some(root) = transcripts_root.as_ref() {
    let pool_cl = pool.clone();
    let tok_cl = cancel.clone();
    let root_cl = root.clone();
    background.spawn(async move {
        if let Err(e) = transcript_tail::run(pool_cl, root_cl, tok_cl).await {
            tracing::error!(error=?e, "transcript tail exited with error");
        }
    });
}
```

### 6.3 Default transcripts root

Reuse `src/paths.rs::default_transcripts_root()` (already exists for slice-1).

### 6.4 Failure cases

| Case | Behaviour |
|---|---|
| `~/.claude/projects` doesn't exist | log warn, tail disabled (don't error serve) |
| File deleted after watcher started | drop cursor, no error |
| File truncated (cursor past EOF) | reset cursor to 0, re-ingest (dedup handles dupes) |
| Parse error on a line | record as `parse_error` in `raw_event` (existing slice-1 behaviour) |
| 1000s of files at startup | seed cursor pass takes seconds; acceptable for MVP |

---

## 7. Doctor v0.2 Implementation

### 7.1 New module structure

`src/doctor.rs` grows to ~700 LOC. Internal split:

- `settings::scopes()` — returns `Vec<SettingsScope>` with `{kind, path, present, parsed: Option<Value>}` for managed (+ drop-in dir), user, project shared, project local. Walks up from `opts.project` (default CWD) to find the project root (`.claude/` present or `.git/` present, capped at root).
- `settings::effective_env(&scopes)` → `BTreeMap<String, EnvSource>` where `EnvSource { value: String, scope: ScopeKind }` reflects precedence.
- `settings::hook_entries(&scopes, plugins_root)` → `Vec<HookEntry { event, command, scope }>` walking each scope plus `~/.claude/plugins/**/*.{json,toml}` for plugin manifests.
- `settings::managed_policy(&scopes)` → flags like `allow_managed_hooks_only`, `disable_all_hooks`.

### 7.2 Settings file paths (OS-aware)

```rust
fn managed_paths() -> Vec<PathBuf> {
    #[cfg(target_os = "macos")]
    let base = PathBuf::from("/Library/Application Support/ClaudeCode");
    #[cfg(target_os = "linux")]
    let base = PathBuf::from("/etc/claude-code");
    #[cfg(target_os = "windows")]
    let base = PathBuf::from(r"C:\Program Files\ClaudeCode");
    let mut out = vec![base.join("managed-settings.json")];
    if let Ok(entries) = std::fs::read_dir(base.join("managed-settings.d")) {
        let mut paths: Vec<_> = entries.flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("json"))
            .collect();
        paths.sort(); // alphabetical merge per docs
        out.extend(paths);
    }
    out
}
```

### 7.3 Plugin manifest discovery

`~/.claude/plugins/` is walked one level deep. Inside each plugin dir, common manifest names: `plugin.json`, `manifest.json`. We parse each and look for a `hooks` block (same schema as settings.json). Any forward to `hooks/v1/events` is reported with `scope = "plugin:<dirname>"`.

Failure case: missing plugins dir → empty list, no error. Malformed manifest → skip with a soft warning in the report (not an exit-1).

### 7.4 Effective env algorithm

For each key in the union of all scopes' env blocks:
1. Pick the highest-precedence scope that defines it (managed > local > project > user).
2. Record `{value, scope}`.
3. Compare against process env. If process env differs (or only process env has it), record both in a `divergence` section.

Report two views:
- **file-effective** — what `claude` will see when launched from this CWD (managed file → user → project → local merge).
- **shell-effective** — what the current shell holds (process env). Differences are flagged.

This collapses the most confusing case the user hit: setting `OTEL_EXPORTER_OTLP_ENDPOINT` only in `~/.claude/settings.json` works (claude injects it) but doctor v0.1 said "unset".

### 7.5 Pretty output sketch

```
witmcc doctor — read-only diagnostic

# Settings files probed
  ∅ managed   /Library/Application Support/ClaudeCode/managed-settings.json
  ✓ user      /Users/x/.claude/settings.json
  ✓ project   /Users/x/projects/lhh-liveops/.claude/settings.json
  ∅ local     /Users/x/projects/lhh-liveops/.claude/settings.local.json

# Effective OTel env (file scope = what `claude` will see)
  ✓ OTEL_EXPORTER_OTLP_ENDPOINT     http://localhost:7878/otel   (user)
  ✓ OTEL_METRICS_EXPORTER            otlp                          (user)
  ✗ OTEL_TRACES_EXPORTER             (not set in any scope) — traces won't be emitted

  shell-effective env (current process) does NOT contain these — that's fine,
  `claude` reads from settings.json and injects into its own subprocess.

# Hook forwarding to witmcc
  ✓ PostToolUse  → /usr/local/bin/witmcc-forward.sh   (user)
  ∅ PreToolUse   no forward registered                — receiver will see only PostToolUse

  Managed policy:
    allowManagedHooksOnly = false   (your user/project hooks WILL load)
    disableAllHooks       = false

# Plugin hooks discovered
  (none in ~/.claude/plugins)

# Server
  ✓ reachable (build_sha = ...)
  source         last_ingested_at        rows/24h    total
  ✓ transcript   2026-05-21T01:30:00Z      412         412
  ✓ otel-metrics 2026-05-21T01:30:05Z       34          34
  ...

exit_code = 0
```

### 7.6 Exit code (unchanged)

`0` when server reachable + at least one source has rows in the last 24h.
`1` otherwise. `--json` always exits 0.

---

## 8. Test Strategy

### 8.1 Transcript tail

`tests/transcript_tail.rs` (new):

- Spawn tail against a tempdir root. Touch a `<sid>.jsonl` file, write one line. Within 1s, expect `observed_event` count > 0 for that session.
- Append a second line, verify only 1 new row inserted (existing lines are not re-ingested).
- Restart the tail (drop + respawn) after the test wrote some lines — cursor seeds from `raw_event`, no duplicate inserts.
- Truncate a file (write 0 bytes) — cursor resets, no error.
- Run alongside `ingest --all` — idempotent.

### 8.2 Doctor v0.2

`tests/doctor.rs` extensions:

- Synthesise a tempdir with `<tempdir>/.claude/settings.json` containing OTel env + hooks → doctor with `--project <tempdir>` finds them and labels source = "project shared".
- Add `<tempdir>/.claude/settings.local.json` with conflicting `OTEL_EXPORTER_OTLP_ENDPOINT` → doctor picks local (higher precedence).
- Create a plugin dir under `<HOME-override>/.claude/plugins/test-plugin/plugin.json` with a hook entry → doctor reports `scope = "plugin:test-plugin"`. (Use a HOME-overriding test util.)
- Process env diverging from file scopes → output mentions both and flags divergence.
- Empty environment + no settings + unreachable server → exit 1, both env and hooks rows show as missing.

### 8.3 Acceptance smoke (manual)

```bash
witmcc serve --auto-migrate
# settings.json already has OTel env + hooks
# new shell:
cd /any/repo && claude   # interactive session, /exit
# back in serve shell — within 5s of /exit, witmcc has:
#   raw_event rows for claude_transcript, otel-metrics, otel-logs, otel, hook
# without running `witmcc ingest --all`.
witmcc doctor   # exit 0; all 6 source rows green; effective env block shows everything
```

---

## 9. Risks & Mitigations

| Risk | Mitigation |
|---|---|
| Transcript watcher misses bursts of writes | 100ms debounce; per-path queue; cursor reload on each touch is cheap because we seek to byte offset |
| Cursor desync if witmcc restarts mid-write | UNIQUE constraint on `raw_event(source_uri, source_line_no, payload_sha256)` absorbs dupes; cursor recovery re-derives from DB |
| Doctor walks expensive paths (`~/.claude/plugins/**`) | One-level deep walk only; bounded by user's plugin count |
| Doctor reads settings.json belonging to other users (test env) | Only HOME + CWD-derived paths; respects `--project` override |
| `~/.claude/projects` not present | log warn + disable tail; doctor reports "no transcripts root found" |
| Existing slice-1 ingest path assumes whole-file reads | Tail path uses `transcript::parse_line` directly; doesn't touch `walk_jsonl` |

---

## 10. Migration Path

1. Existing slice-6 users `git pull`; no migration needed (SCHEMA_VERSION unchanged).
2. `witmcc serve` now spawns transcript tail by default. Users wanting old behaviour pass `--no-watch-transcripts`.
3. `witmcc doctor` output gains new sections — JSON consumers may see new keys but no key is removed.
4. README updated with the simplified one-command path (`witmcc serve` + Claude Code env in settings.json + one hook forward).

---

## 11. Acceptance Criteria for Slice-7

1. `witmcc serve --auto-migrate` (no other commands) followed by a `claude` session causes `transcript` `source_type` rows to appear in `raw_event` within 5s of new JSONL lines being written.
2. Restarting witmcc does not re-ingest already-seen transcript lines (idempotency via existing unique constraint + cursor re-derivation).
3. `witmcc doctor` reports the correct scope for each OTel env variable when defined in *any* of {managed, user, project, local}.
4. Plugin manifests under `~/.claude/plugins/**/plugin.json` (or `manifest.json`) that contain a hook forwarding to `hooks/v1/events` appear in doctor's "Plugin hooks discovered" section.
5. `allowManagedHooksOnly = true` in a managed settings file causes doctor to warn that user/project hooks are silenced.
6. All existing cargo + vitest tests pass; +8 cargo tests minimum (5 tail, 3 doctor v0.2 extensions).
7. README's "one-command" path works end-to-end: `serve` + settings.json + one hook forward → all 6 sources green in doctor within 60s of `/exit`.
8. `docs/implementation-notes.html` gains a `slice-7` section with the deviations actually encountered.

---

## 12. Open Decisions (resolved for this slice)

| Decision | Choice | Rationale |
|---|---|---|
| Cursor tracking | In-memory `HashMap<PathBuf, u64 byte_offset>` seeded from `raw_event` on startup | No new table; UNIQUE constraint absorbs corruption; reload on restart is cheap |
| Default for transcript tail | ON | Slice exists to make "one command" work; opt-out via `--no-watch-transcripts` |
| Transcripts root | `~/.claude/projects` (autodetect via `paths::default_transcripts_root`); `--transcripts-root` override | Matches slice-1 |
| Hook auto-install | Still NO | CLAUDE.md non-goal; doctor outputs snippets only |
| Plugin manifest schema parsing | Substring-match on `hooks/v1/events` only | Plugin schema is fluid; rich parsing is a separate slice |
| Settings file watching for live config changes | Out of scope | Doctor is on-demand; live reload would mean re-reading on every request |
| Cross-source dedup of `hook_execution_complete` (otel-logs ↔ hook ↔ transcript) | Still out of scope | DEV-S6-06 stands |

---

## 13. Follow-ups unblocked by this work

- **Findings engine (M5)** — with transcript live + OTel live + hook + file/git all in real-time, the missing-verification / tool-failure detectors finally have continuous evidence to consume.
- **Cross-source dedup** — same `(session_id, hook_event_name, tool_use_id)` may now arrive via three paths; collapse policy can be specced once live data shape is observable.
- **Plugin manifest first-class support** — schema validation, plugin-as-source-of-truth for hook entries.
- **Settings hot-reload** — `notify` watcher on `settings.json` files to re-evaluate doctor's snapshot continuously.
