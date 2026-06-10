# wimcc — What's in My Claude Code

**English** · [한국어](README.ko.md)

**Replay what Claude Code actually did — not what it said.**

## What's in my cc?

wimcc replays a Claude Code session — the one that just ran, or the one running
now — as an execution you can step through. It surfaces what the chat log alone
doesn't: which tool failed and why, how much time and how many tokens a model
request cost, what a hook blocked, which edit changed which lines of which file
— all on one screen, each item traceable straight back to the raw record it
came from.

Things you can do with wimcc:

- See **which tool calls failed in this session, and why**.
- Track **where the tokens went** — usage, cost, and context efficiency.
- Trace **which edit changed which lines of which file** (file lineage).
- Check **what a hook blocked or let through**.
- Follow **how model requests, tool calls, and hooks interleaved** over time.
- Read all of the above in a browser UI — or pull it into another tool or agent
  over the **Pull API / MCP**.

Everything runs locally on `127.0.0.1` and nothing is sent outward. External
access is read-only.

## Quick start

```bash
just build-release                            # build the SPA + release binary (target/release/wimcc) in one step
./target/release/wimcc init-db                # apply migrations, prepare .wimcc.sqlite
./target/release/wimcc serve --auto-migrate   # http://127.0.0.1:7878  (auth off by default)
./target/release/wimcc doctor                 # verify collector wiring
```

`serve` runs everything in one process: the read-only Pull API, the embedded
WebUI, the OTel + hook receivers, and a live tail of `~/.claude/projects`
transcripts. For Claude Code to emit OTel + hook events into wimcc you set up
`~/.claude/settings.json` once — see [Connecting Claude Code](#connecting-claude-code).
`wimcc doctor` tells you what's connected and what's missing.

`wimcc ingest --all` is still available for backfill (a cold-start sweep of
existing transcript JSONLs) but is not required for live operation.

## CLI

```
wimcc [--db-path <PATH>] [--log-format pretty|json] [--verbose] <command>
```

Global flags apply to every subcommand. `--db-path` defaults to
`.wimcc.sqlite` (env `WIMCC_DB`).

| Command | Purpose |
| --- | --- |
| `init-db` | Apply migrations and prepare the database. |
| `ingest --all` / `ingest --path <P>` | Backfill: scan transcript JSONL files into raw + observed events (idempotent). |
| `doctor [--json] [--server <URL>] [--project <DIR>]` | Read-only diagnosis of collector wiring (settings hierarchy, hooks, server probe). Never mutates anything. |
| `serve` | Start the local service: Pull API + WebUI + OTel/hook receivers + transcript live tail. |

### `wimcc serve` flags

```
wimcc serve [--bind 127.0.0.1] [--port 7878]
             [--auto-migrate]                    # apply pending migrations on startup
             [--transcripts-root <PATH>]         # override ~/.claude/projects
             [--no-watch-transcripts]            # disable the live tail (OTel/hook only)
             [--auth off|on]                      # bearer-token auth on /v1 + /mcp (default: off)
             [--retention-profile none|default|strict]   # background retention sweep (default: none)
             [--print-token] [--rotate-token]     # manage the bearer token, then exit
             [--sse-keepalive-secs N]             # WebUI live-stream keep-alive (default: 30)
             [--sse-channel-capacity N]           # broadcast channel capacity (default: 512)
             [--shutdown-after-ms N]              # test/smoke convenience
```

## What it observes

| Source | How it arrives | Notes |
| --- | --- | --- |
| **Transcript** | live tail of `~/.claude/projects/**/*.jsonl` (or `ingest` backfill) | user/assistant messages, tool calls + results, thinking, attachments |
| **OTel traces / metrics / logs** | `POST /otel/v1/*` from Claude Code's OTLP exporter | traces are beta (`CLAUDE_CODE_ENHANCED_TELEMETRY_BETA=1`) |
| **Hook lifecycle** | `POST /hooks/v1/events` from a forward script | nine `hook_event_name`s recognised; unknown names ingest as `subkind="unknown"` |
| **Edit diffs** | extracted from each transcript tool-result's `toolUseResult.structuredPatch` | only `Edit` produces hunks; `Write` emits an empty patch. Powers `/v1/sessions/:id/diff-hunks` and the `get_file_lineage` MCP tool |
| **Verification runs / token usage** | derived from transcript + telemetry facets | surface via the verification-run and usage endpoints |

## Endpoints

All `/v1/*` and `/mcp` responses are wrapped in
`{meta: {schema_version, collection_profile, generated_at, ...}, data: ...}`.
When `--auth on`, every `/v1/*` and `/mcp` request needs
`Authorization: Bearer <token>`; the OTel/hook collectors and the SSE stream are
always unauthenticated loopback endpoints.

### Read-only Pull API (`GET`)

| Path | Response |
| --- | --- |
| `/v1/health` | `{status, build_sha}` |
| `/v1/health/sources` | per-source freshness (used by `doctor`) |
| `/v1/sessions` | session list (newest first) |
| `/v1/sessions/{id}` | `{summary, events[]}` |
| `/v1/sessions/{id}/events` | paged observed events |
| `/v1/sessions/{id}/diff-hunks` | edit hunks for the session |
| `/v1/sessions/{id}/usage` | token-usage rollup (`assistant_events`, `user_turns`, tokens, estimated cost) |
| `/v1/sessions/{id}/metrics` | on-demand session behavioral metrics — composable counts only (no rates) |
| `/v1/sessions/{id}/signals` | deterministic detector signals (evidence-linked) |
| `/v1/signals/{id}` | a single signal |
| `/v1/sessions/{id}/verification-runs` | verification runs in the session |
| `/v1/verification-runs/{id}` | a single verification run |
| `/v1/usage/baseline` | cross-session usage baseline (p25/median/p75) |
| `/v1/detectors` | detector manifest (5 deterministic L1 detectors) |
| `/v1/events/{event_id}/raw` | source-preserving raw payload of one event |
| `/v1/audit` | audit log |
| `/v1/stream` | Server-Sent Events live stream (drives the WebUI) |

### Collectors (`POST`, loopback, unauthenticated)

| Path | Signal |
| --- | --- |
| `/otel/v1/traces` | OTel traces (beta) |
| `/otel/v1/metrics` | OTel metrics |
| `/otel/v1/logs` | OTel logs |
| `/hooks/v1/events` | hook lifecycle events (single object or array, ≤ 1 MB) |

OTLP bodies are OTLP/JSON, gzip optional, ≤ 4 MB. The `/otel` prefix is
**required** — without it the OTel SDK posts to `…/v1/metrics` and wimcc
returns 404.

### MCP (Streamable HTTP)

`POST`/`GET /mcp` exposes the same read-only data as MCP tools:

- `whats_in_my_cc.search_sessions`
- `whats_in_my_cc.get_file_lineage`
- `whats_in_my_cc.get_otel_trace`
- `whats_in_my_cc.list_detectors`

## Web UI

The `wimcc` binary embeds a React SPA (rust-embed), served at
`http://127.0.0.1:7878/`. Two pages:

- `/sessions` — session list
- `/sessions/:id` — event-first replay: a conversation stream with per-event
  detail panel, raw-source tab, an insight strip (context efficiency / tokens /
  verification / tool failures / cost, with provenance badges), an analysis
  panel (session metrics + detector firing), and an untagged-Bash panel for the
  tagging loop.

For local development (dev servers, builds, tests) see
[Build, test, develop](#build-test-develop).

## Connecting Claude Code

wimcc doesn't modify `settings.json` automatically — add the blocks below once,
then run `wimcc doctor` to confirm scope attribution.

### OTel

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

Traces are beta — without `CLAUDE_CODE_ENHANCED_TELEMETRY_BETA=1` the SDK never
emits spans. Records without `session.id` are stored but excluded from
`/v1/sessions`.

### Hooks

Register a forward script for the lifecycle events you care about
(`PreToolUse`, `PostToolUse`, `UserPromptSubmit`, `Stop`, `SubagentStop`,
`Notification`, `PreCompact`, `SessionStart`, `SessionEnd`):

```jsonc
{
  "hooks": {
    "PreToolUse":  [{ "hooks": [{ "type": "command", "command": "/usr/local/bin/wimcc-forward.sh" }] }],
    "PostToolUse": [{ "hooks": [{ "type": "command", "command": "/usr/local/bin/wimcc-forward.sh" }] }]
    // … repeat for the other events
  }
}
```

`/usr/local/bin/wimcc-forward.sh`:

```bash
#!/bin/bash
exec curl -sS -m 2 -X POST \
  -H 'content-type: application/json' \
  --data-binary @- \
  http://127.0.0.1:7878/hooks/v1/events > /dev/null 2>&1 || true
```

`-m 2` + `|| true` gives **fail-soft degrade semantics** (PRD OBS-3): if wimcc
is down or slow, your Claude Code session is never blocked.

### Smoke tests

```bash
curl -X POST http://127.0.0.1:7878/otel/v1/metrics \
  -H 'Content-Type: application/json' \
  --data-binary @tests/fixtures/otel/real/metrics_v01.json

curl -X POST http://127.0.0.1:7878/hooks/v1/events \
  -H 'content-type: application/json' \
  --data-binary @tests/fixtures/hook/pre_tool_use.json
```

## Auth & retention

- **Auth** defaults to `off` (single-user local dev) — open the browser
  directly. `wimcc serve --auth on` enforces a bearer token on `/v1/*` + `/mcp`.
  Token file: macOS `~/Library/Application Support/wimcc/token`, Linux
  `~/.config/wimcc/token` (mode `0600`). Manage it with `serve --print-token`
  / `--rotate-token`.
- **Retention** defaults to `none` (no deletion). `--retention-profile default`
  (30d/180d/90d) or `strict` (7d/30d/30d) enables a background sweep.

## Security notes

- **Ingest-time redaction** masks known secret patterns (rule pack v1) before
  raw payloads are stored, recording a `redaction_manifest` per event.
  High-entropy strings are flagged only, and there is no export-side review —
  still treat the SQLite file and anything reachable on `127.0.0.1` as
  sensitive.
- Edit-hunk text is truncated; binary diffs surface as `<binary>`.
- The OTel real-fixture freeze script auto-redacts PII to stable placeholders,
  but always grep for your own email before committing fixtures.

## Build, test, develop

Build and test are wrapped as `just` recipes. The backend binary embeds
`webui/dist/` at compile time (rust-embed), so **the SPA must be built before you
build or test the backend.**

| Recipe | What it does |
| --- | --- |
| `just webui-install` | install webui npm deps (idempotent) |
| `just webui-build` | production SPA build → `webui/dist/` (`tsc -b && vite build`) |
| `just webui-test` | frontend unit tests (`vitest run`) |
| `just webui-dev` | vite dev server (`127.0.0.1:5173`, proxies `/v1` · `/otel` · `/hooks` → `7878`) |
| `just serve-dev` | run the backend in dev (`cargo run -- serve --auto-migrate`) |
| `just build-release` | `webui-build`, then `cargo build --release` → `target/release/wimcc` |

**Release build** — a single binary with the WebUI embedded:

```
just build-release
./target/release/wimcc serve --auto-migrate
```

**Backend tests** — `cargo test`. rust-embed needs `webui/dist/` at compile
time, so on a fresh clone build the SPA once first:

```
just webui-build
cargo test
```

**Frontend tests** — `just webui-test` (`vitest run`). Watch mode:
`cd webui && npm run test:watch`.

**Frontend dev loop** — run both processes: `just serve-dev` (backend) and
`just webui-dev` (vite with HMR), then open `http://127.0.0.1:5173`.

**Node version** — Node 20 for the build (`webui/.nvmrc`). The untagged-Bash
tooling script (`webui/scripts/untagged-bash.ts`) needs Node 22+ for native type
stripping.

**dev DB regeneration** — after a migration change (latest `0022`), run
`wimcc init-db` and re-ingest. Payload fields stored as JSON BLOBs
(`tool_call.tool_name`, `assistant_message.model`, …) are added without a schema
migration, so existing events won't have them until re-ingested.

## Reference docs

- Full system specs: `docs/index.html` and `docs/00..05_*.html`
- Implementation notes (deviations, decisions, event-first redesign):
  `docs/implementation-notes.html`
- Project guidance for contributors: `CLAUDE.md`

