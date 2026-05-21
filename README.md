# witmcc — What's in My Claude Code (slice-1)

Local-only inspection of Claude Code execution. **Slice-1 ships:** transcript
JSONL ingest → SQLite → deterministic-edge session graph → 127.0.0.1 read-only Pull API.

Out of slice-1 (later slices): OTel/Hook/File-Git collectors, UI, MCP, redaction, auth.

## Quick start

```bash
cargo run -- init-db
cargo run -- ingest --all                     # scans ~/.claude/projects/**/*.jsonl
cargo run -- serve                            # 127.0.0.1:7878
curl http://127.0.0.1:7878/v1/health
curl http://127.0.0.1:7878/v1/sessions | jq .
```

## Endpoints

| GET path | response |
| --- | --- |
| `/v1/health` | `{status, build_sha}` |
| `/v1/sessions` | list of `{session_id, first_observed_at, last_observed_at, event_count}` |
| `/v1/sessions/{id}` | `{summary, events[]}` |
| `/v1/sessions/{id}/graph` | `{nodes[], edges[]}` |

All non-health responses are wrapped in `{meta: {schema_version, collection_profile, generated_at, ...}, data: ...}`.

## Tests

`cargo test` requires `webui/dist/` to be present at compile time (rust-embed
embeds it). On a fresh clone, build the SPA once first:

```
just webui-build
cargo test
just webui-test    # frontend unit tests (vitest)
```

## Known limits in slice-1

- No redaction. Do not point at JSONL files that may contain secrets you're unwilling to expose to anything that can reach 127.0.0.1.
- No live tail. Re-run `ingest` to pick up newly appended JSONL lines (idempotent).
- `tool_call_to_result` edges appear as a self-loop (`from == to == tool_call`) with `attributes.merged = true` for matched calls; dangling tool_results get a regular call→result edge.
- `last-prompt`, `permission-mode`, `file-history-snapshot`, `thinking`, and non-hook `attachment` events are preserved as ObservedEvents but do not get their own graph nodes.

## Web UI (slice-2)

The `witmcc` binary embeds a small React SPA at runtime. Build it once before
`cargo build`:

```
just webui-build      # cd webui && npm install && npm run build
just build-release    # cargo build --release
./target/release/witmcc serve --auto-migrate
# then open http://127.0.0.1:7878/
```

For frontend-only iteration:

```
just serve-dev        # axum on 127.0.0.1:7878
just webui-dev        # vite on 127.0.0.1:5173, proxies /v1 → 7878
```

The SPA has two pages:

- `/sessions` — session list
- `/sessions/:id` — six-lane timeline + raw source panel

Node 20 is required; see `webui/.nvmrc`.

### OTel Receivers (slice-3 traces, slice-6 metrics + logs)

witmcc accepts all three Claude Code OTel signals at a single loopback origin:

| Endpoint | Signal | Notes |
|---|---|---|
| `POST /otel/v1/metrics` | metrics (slice-6) | OTLP/JSON, gzip optional, ≤4 MB |
| `POST /otel/v1/logs`    | logs (slice-6)    | OTLP/JSON, gzip optional, ≤4 MB |
| `POST /otel/v1/traces`  | traces (slice-3, beta) | requires `CLAUDE_CODE_ENHANCED_TELEMETRY_BETA=1` |

Receiver is two-stage: raw OTLP body is persisted verbatim into `raw_event` first
(source-preserving), then normalised into per-data-point `MetricSample` and
per-record `LogRecord` `ObservedEvent` rows + graph nodes (slice-6 Stage 2).
Each signal also surfaces on the `OTel` lane in the WebUI with kind-specific
markers (indigo dashed = span, sky-blue = metric, amber = log).

#### Wire Claude Code via `~/.claude/settings.json`

```jsonc
{
  "env": {
    "CLAUDE_CODE_ENABLE_TELEMETRY": "1",
    "CLAUDE_CODE_ENHANCED_TELEMETRY_BETA": "1",
    "OTEL_METRICS_EXPORTER": "otlp",
    "OTEL_LOGS_EXPORTER":    "otlp",
    "OTEL_TRACES_EXPORTER":  "otlp",
    "OTEL_EXPORTER_OTLP_PROTOCOL": "http/json",
    "OTEL_EXPORTER_OTLP_ENDPOINT": "http://localhost:7878/otel",
    "OTEL_METRIC_EXPORT_INTERVAL": "5000",
    "OTEL_LOGS_EXPORT_INTERVAL":   "2000",
    "OTEL_TRACES_EXPORT_INTERVAL": "2000"
  }
}
```

The `/otel` suffix on the endpoint is **required** — without it the OTel SDK
posts to `…/v1/metrics` instead of `…/otel/v1/metrics` and witmcc returns 404.
witmcc does **not** auto-edit settings.json (CLAUDE.md non-goal).

Manual smoke (any signal):

```bash
curl -X POST http://127.0.0.1:7878/otel/v1/metrics \
  -H 'Content-Type: application/json' \
  --data-binary @tests/fixtures/otel/real/metrics_v01.json
```

#### Re-freeze real fixtures

After a noteworthy Claude Code release, refresh the v01 fixtures to keep
parser anchors on real bytes:

```bash
./target/release/witmcc serve --auto-migrate &
cd /any/repo && claude   # short interactive session, /exit
python3 scripts/freeze_real_otel_fixtures.py
# inspect + commit the diff in tests/fixtures/otel/real/
```

PII (user.email / user.id / *.account_* / organization.id / session.id) is
auto-redacted to stable placeholders by the freeze script. Always grep for
your own email before committing.

Notes:
- Traces are beta in Claude Code — without `CLAUDE_CODE_ENHANCED_TELEMETRY_BETA=1`
  the SDK never emits spans.
- No redaction (M7). Logs in particular carry user prompts and tool input;
  treat the SQLite file as sensitive until M7 ships.
- Records without `session.id` are stored but excluded from `/v1/sessions`.

### Doctor (slice-6)

```bash
witmcc doctor            # pretty table, exits 0 when collectors are healthy
witmcc doctor --json     # structured report for tooling, always exits 0
witmcc doctor --server http://127.0.0.1:7878
```

Reports OTel env vars, hook settings wiring, and per-source `last_ingested_at`
from `GET /v1/health/sources`. Pure read-only — never writes settings.json or
shell env. Missing items are printed as copy-pastable snippets at the bottom.

### Hook Collector (slice-4)

`POST /hooks/v1/events` accepts Claude Code hook lifecycle events directly. The
body is a single hook JSON object **or** a JSON array of hook objects (≤ 1 MB).
Nine `hook_event_name` values are recognised (`PreToolUse`, `PostToolUse`,
`UserPromptSubmit`, `Stop`, `SubagentStop`, `Notification`, `PreCompact`,
`SessionStart`, `SessionEnd`); unknown names ingest with `subkind="unknown"`.

Wire it up via a forward script registered in `~/.claude/settings.json`:

```jsonc
{
  "hooks": {
    "PreToolUse":  [{ "hooks": [{ "type": "command", "command": "/usr/local/bin/witmcc-forward.sh" }] }],
    "PostToolUse": [{ "hooks": [{ "type": "command", "command": "/usr/local/bin/witmcc-forward.sh" }] }],
    "UserPromptSubmit": [{ "hooks": [{ "type": "command", "command": "/usr/local/bin/witmcc-forward.sh" }] }],
    "Notification": [{ "hooks": [{ "type": "command", "command": "/usr/local/bin/witmcc-forward.sh" }] }],
    "PreCompact":   [{ "hooks": [{ "type": "command", "command": "/usr/local/bin/witmcc-forward.sh" }] }],
    "SessionStart": [{ "hooks": [{ "type": "command", "command": "/usr/local/bin/witmcc-forward.sh" }] }],
    "SessionEnd":   [{ "hooks": [{ "type": "command", "command": "/usr/local/bin/witmcc-forward.sh" }] }],
    "Stop":         [{ "hooks": [{ "type": "command", "command": "/usr/local/bin/witmcc-forward.sh" }] }],
    "SubagentStop": [{ "hooks": [{ "type": "command", "command": "/usr/local/bin/witmcc-forward.sh" }] }]
  }
}
```

`/usr/local/bin/witmcc-forward.sh`:

```bash
#!/bin/bash
exec curl -sS -m 2 -X POST \
  -H 'content-type: application/json' \
  --data-binary @- \
  http://127.0.0.1:7878/hooks/v1/events > /dev/null 2>&1 || true
```

`-m 2` (2 second timeout) combined with `|| true` implements **fail-soft degrade
semantics** (PRD OBS-3): if the witmcc receiver is down, slow, or unreachable,
your Claude Code session is never blocked.

Manual smoke test:

```bash
curl -X POST http://127.0.0.1:7878/hooks/v1/events \
  -H 'content-type: application/json' \
  --data-binary @tests/fixtures/hook/pre_tool_use.json
```

Notes:
- witmcc does **not** install the forward script automatically (CLAUDE.md
  non-goal: "Claude Code 설정 / hook / command / skill / memory 변경").
- Hook payloads can carry secrets (prompt text, command output, `tool_input`);
  redaction is M7. Only enable forwarding in trusted contexts until then.
- External hook events appear on the new `Hook` lane in the UI.

### File/Git observer (slice-5)

`witmcc serve --watch <path>` spawns two background tokio tasks alongside the
HTTP server. A filesystem watcher (`notify` 7.x) emits debounced `file_event`
records on create / modify / delete / rename. If `<path>/.git` exists, a git
poller (default 5 s) emits one `git_commit` + one `diff_hunk` per hunk on every
new commit. Hunks are also persisted in a dedicated `diff_hunk` side-table for
the spec-defined `file_lineage_idx` (migration `0003`).

All file/git events live on a synthetic `session_id = "filesystem"` — they
surface through the same `/v1/sessions` endpoint as transcript / OTel / hook
sessions, and the SPA's new `Files` lane (8th) renders the three new node kinds.

Smoke:

```bash
mkdir -p /tmp/witmcc-smoke && (cd /tmp/witmcc-smoke && git init && touch a.txt && \
  git -c user.email=t@t -c user.name=t commit --allow-empty -m init)
./target/release/witmcc serve --bind 127.0.0.1 --port 7878 \
  --watch /tmp/witmcc-smoke --git-poll-secs 1 --auto-migrate &
sleep 1
echo hello > /tmp/witmcc-smoke/a.txt
(cd /tmp/witmcc-smoke && git add . && git -c user.email=t@t -c user.name=t commit -m bump)
sleep 3
curl -sS http://127.0.0.1:7878/v1/sessions/filesystem/graph | \
  jq '.data.nodes[].node_kind' | sort -u
# expect: diff_hunk, file_event, git_commit
```

Flags:

- `--watch <path>` — directory to observe; both collectors disabled if the path is missing.
- `--git-poll-secs N` — polling interval (default `5`, minimum `1`).
- `--shutdown-after-ms N` — test/smoke convenience: auto-shutdown after `N` ms.

Known limits in slice-5:

- File mutations between commits surface as `file_event` only — there is no
  per-file content diff. Hunks come from `git commit` diffs only.
- Hunk text is truncated to 4 KB per hunk. Binary diffs surface with
  `line_range_after = null` and `patch_preview = "<binary>"`.
- `MAX_HUNKS_PER_COMMIT = 2000`; surplus hunks are dropped (and counted in the ingest result).
- No redaction (M7). Hunks may carry secrets.
- `session_id="filesystem"` is reserved.
- Watcher applies a hardcoded system-default ignore list (`src/watcher.rs` →
  `default_ignore` module): VCS metadata (`.git`, `.hg`, `.svn`, `.bzr`),
  `target/`, macOS metadata (`.DS_Store`, `.Spotlight-V100/`, `.fseventsd/`,
  `.Trashes/`, `.TemporaryItems/`), Windows metadata (`Thumbs.db`, `desktop.ini`),
  all SQLite sidecars (`*.sqlite`, `*.sqlite-*`), and common editor temp/swap
  files (`*.swp`, `*.swo`, `4913`, `.#*`, `*~`). The list is NOT user-configurable
  in slice-5; service-specific `.witmccignore` is a follow-up.
- The git poller on startup uses the current `HEAD` as `last_seen` — commits made before `serve` started are not back-filled.

## Reference docs

- Spec (this slice): `docs/superpowers/specs/2026-05-19-witmcc-slice1-transcript-design.md`
- Plan: `docs/superpowers/plans/2026-05-19-witmcc-slice1-transcript.md`
- Full system docs: `docs/index.html` and `docs/00..06_*.html`
- Implementation notes (deviations & decisions): `docs/implementation-notes.html`
