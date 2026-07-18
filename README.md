# wimcc — What's in My Claude Code

[![CI](https://github.com/bahamoth/whats-in-my-cc/actions/workflows/ci.yml/badge.svg)](https://github.com/bahamoth/whats-in-my-cc/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/bahamoth/whats-in-my-cc)](https://github.com/bahamoth/whats-in-my-cc/releases/latest)

**English** · [한국어](README.ko.md)

**Replay what Claude Code actually did. Every step visible to both humans and agents.**

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

Use any of the channels in [Install](#install) below. The shell installer is
the fastest path — it downloads the right dist archive for your platform
automatically (macOS Apple Silicon/Intel, Linux x86_64/aarch64 musl). The
WebUI is embedded; the single binary is all you need.

```bash
curl -fsSL https://github.com/bahamoth/whats-in-my-cc/releases/latest/download/wimcc-installer.sh | sh
wimcc init-db                # apply migrations, prepare .wimcc.sqlite
wimcc serve --auto-migrate   # http://127.0.0.1:7878  (auth off by default)
wimcc doctor                 # verify collector wiring
```

## Install

```sh
# shell (macOS / Linux)
curl -fsSL https://github.com/bahamoth/whats-in-my-cc/releases/latest/download/wimcc-installer.sh | sh

# Homebrew
brew install bahamoth/tap/wimcc

# npm (Claude Code users already have this)
npm install -g wimcc

# cargo
cargo install wimcc          # or: cargo binstall wimcc

# mise
mise use -g ubi:bahamoth/whats-in-my-cc
```

## Update

- shell install: `wimcc self-update` (check only: `wimcc self-update --check`)
- brew / npm / cargo: use that manager's upgrade command
- The running `wimcc serve` keeps the old binary until restarted — restart when
  no live Claude Code session is being observed: `wimcc service restart`
- `wimcc serve` checks GitHub Releases metadata once a day (its only outbound
  call) and shows a banner in the WebUI; disable with `--update-check off`
  or `WIMCC_UPDATE_CHECK=off`

## Run as a service

```sh
wimcc service install    # start on login (launchd / systemd --user)
wimcc service status
wimcc service restart    # e.g. after self-update
wimcc service uninstall
```

### Build from source

```bash
just build-release                            # build the SPA + release binary (target/release/wimcc) in one step
./target/release/wimcc init-db                # apply migrations, prepare .wimcc.sqlite
./target/release/wimcc serve --auto-migrate   # http://127.0.0.1:7878  (auth off by default)
./target/release/wimcc doctor                 # verify collector wiring
```

`serve` runs everything in one process: the read-only Pull API, the embedded
WebUI, the OTel receiver, and a live tail of `~/.claude/projects`
transcripts. For Claude Code to emit OTel events into wimcc you set up
`~/.claude/settings.json` once — see [Connecting Claude Code](#connecting-claude-code).
`wimcc doctor` tells you what's connected and what's missing.

`wimcc ingest --all` is still available for backfill (a cold-start sweep of
existing transcript JSONLs) but is not required for live operation.

### Dev environment

For development with hot reload, `just dev` runs the backend and the vite dev
server together:

```bash
just dev   # backend :7878 + vite :5173 (HMR); Ctrl-C stops both
```

Open `http://127.0.0.1:5173`. Full recipe list — tests, builds, single-process
runs — in [Build, test, develop](#build-test-develop).

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
| `doctor [--json] [--server <URL>] [--project <DIR>]` | Read-only diagnosis of collector wiring (settings hierarchy, OTel env, server probe). Never mutates anything. |
| `serve` | Start the local service: Pull API + WebUI + OTel receiver + transcript live tail. |
| `self-update [--check]` | Replace the binary with the latest release (shell installs; package-manager installs are pointed to their manager). Never restarts a running serve. |
| `service install\|uninstall\|restart\|status` | Register serve as a login service (launchd / systemd `--user`). `install` takes `--bind` / `--port` / `--auto-migrate`. |

### `wimcc serve` flags

```
wimcc serve [--bind 127.0.0.1] [--port 7878]
             [--auto-migrate]                    # apply pending migrations on startup
             [--transcripts-root <PATH>]         # override ~/.claude/projects
             [--no-watch-transcripts]            # disable the live tail (OTel/hook only)
             [--auth off|on]                      # bearer-token auth on /v1 + /mcp (default: off)
             [--retention-profile none|default|strict]   # background retention sweep (default: default)
             [--update-check on|off]              # daily GitHub Releases version check (default: on)
             [--print-token] [--rotate-token]     # manage the bearer token, then exit
             [--sse-keepalive-secs N]             # WebUI live-stream keep-alive (default: 30)
             [--sse-channel-capacity N]           # broadcast channel capacity (default: 512)
             [--shutdown-after-ms N]              # test/smoke convenience
```

## What it observes

| Source | How it arrives | Notes |
| --- | --- | --- |
| **Transcript** | live tail of `~/.claude/projects/**/*.jsonl` (or `ingest` backfill) | user/assistant messages, tool calls + results, thinking, attachments, hook results |
| **OTel traces / metrics / logs** | `POST /otel/v1/*` from Claude Code's OTLP exporter | traces are beta (`CLAUDE_CODE_ENHANCED_TELEMETRY_BETA=1`) |
| **Edit diffs** | extracted from each transcript tool-result's `toolUseResult.structuredPatch` | only `Edit` produces hunks; `Write` emits an empty patch. Powers `/v1/sessions/:id/diff-hunks` and the `get_file_lineage` MCP tool |
| **Verification runs / token usage** | derived from transcript + telemetry facets | surface via the verification-run and usage endpoints |

## Endpoints

Most `GET /v1/*` responses are wrapped in
`{meta: {schema_version, collection_profile, redaction_policy, …}, data}` —
with exceptions: `/v1/health` returns bare JSON; the metrics, signals,
detectors, and audit endpoints return `{data}` only; MCP tool results are
JSON-RPC content.
When `--auth on`, every `/v1/*` and `/mcp` request needs
`Authorization: Bearer <token>`; the OTel collectors and the SSE stream are
always unauthenticated loopback endpoints.

### Read-only Pull API (`GET`)

| Path | Response |
| --- | --- |
| `/v1/health` | `{status, build_sha, security: {auth_required, retention_profile}}` |
| `/v1/health/sources` | per-source freshness (used by `doctor`) |
| `/v1/sessions` | session list (newest first) |
| `/v1/sessions/{id}` | `{session_id, summary}` (events come from `/v1/sessions/{id}/events`) |
| `/v1/sessions/{id}/events` | paged observed events |
| `/v1/sessions/{id}/turns` | per-user-turn rollup (tool histogram, edited files, cross-turn file churn, per-turn tokens) |
| `/v1/sessions/{id}/diff-hunks` | edit hunks for the session |
| `/v1/sessions/{id}/usage` | token-usage rollup (`assistant_events`, `user_turns`, tokens, estimated cost) |
| `/v1/sessions/{id}/metrics` | on-demand session behavioral metrics — composable counts only (no rates) |
| `/v1/sessions/{id}/fingerprint` | session environment fingerprint (models, CC versions, git branches, cwd, entrypoint, CLAUDE.md hash) |
| `/v1/sessions/{id}/signals` | deterministic detector signals (evidence-linked) |
| `/v1/signals/{id}` | a single signal |
| `/v1/sessions/{id}/verification-runs` | verification runs in the session |
| `/v1/verification-runs/{id}` | a single verification run |
| `/v1/usage/baseline` | cross-session usage baseline (p25/median/p75) |
| `/v1/metrics` | cross-session metrics + fingerprint series (`project`/`from`/`to`/`limit` filters) — before/after comparison surface |
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

OTLP bodies are OTLP/JSON, gzip optional, ≤ 4 MB. The `/otel` prefix is
**required** — without it the OTel SDK posts to `…/v1/metrics` and wimcc
returns 404.

### MCP (Streamable HTTP)

`POST`/`GET /mcp` exposes the same read-only data as MCP tools:

- `whats_in_my_cc.search_sessions`
- `whats_in_my_cc.get_file_lineage`
- `whats_in_my_cc.get_otel_trace`
- `whats_in_my_cc.get_session_turns`
- `whats_in_my_cc.get_project_metrics`
- `whats_in_my_cc.get_session_metrics`
- `whats_in_my_cc.get_session_signals`
- `whats_in_my_cc.get_session_fingerprint`
- `whats_in_my_cc.list_detectors`

It also serves MCP resources: a per-session summary plus file-lineage and
OTel-trace resource templates.

## Web UI

The `wimcc` binary embeds a React SPA (rust-embed), served at
`http://127.0.0.1:7878/`. Two pages:

- `/sessions` — session list
- `/sessions/:id` — event-first replay: a conversation stream with per-event
  detail panel, raw-source tab, an insight strip (context efficiency / tokens /
  verification / tool failures / cost, with provenance badges), an analysis
  panel (session metrics + detector firing distribution), and an untagged-Bash
  panel for the tagging loop.

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

> Hook lifecycle events are captured from the transcript live tail (hook
> results land in the transcript), so no forward-script wiring is needed. The
> `/hooks/v1/events` collector was removed in 2026-06 — see
> `docs/implementation-notes.html`. On a non-default port, set `WIMCC_PORT` —
> the `session-retrospect` plugin's MCP connection follows it (e.g. `WIMCC_PORT=9000`).

### Smoke test

```bash
curl -X POST http://127.0.0.1:7878/otel/v1/metrics \
  -H 'Content-Type: application/json' \
  --data-binary @tests/fixtures/otel/real/metrics_v01.json
```

## Auth & retention

- **Auth** defaults to `off` (single-user local dev) — open the browser
  directly. `wimcc serve --auth on` enforces a bearer token on `/v1/*` + `/mcp`.
  Token file: macOS `~/Library/Application Support/wimcc/token`, Linux
  `~/.config/wimcc/token` (mode `0600`). Manage it with `serve --print-token`
  / `--rotate-token`.
- **Retention** defaults to `none` (no deletion). `--retention-profile default`
  (raw 30d / normalized 180d / insight 180d / audit 90d) or `strict`
  (raw 7d / normalized 30d / insight 30d / audit 30d) enables a background
  sweep.

## Security notes

- **Ingest-time redaction** masks known secret patterns (rule pack v1) before
  raw payloads are stored, recording a `redaction_manifest` per event.
  High-entropy strings are flagged only, and there is no export-side review —
  still treat the SQLite file and anything reachable on `127.0.0.1` as
  sensitive.
- Diff hunks are derived only from transcript `structuredPatch` text; long
  patch previews are truncated to a bounded size.
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
| `just webui-dev` | vite dev server (`127.0.0.1:5173`, proxies `/v1` → `7878`) |
| `just serve-dev` | run the backend in dev (`cargo run -- serve --auto-migrate`) |
| `just dev` | bring up the full dev environment — backend + vite (HMR) together; Ctrl-C stops both |
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

**Dev environment** — `just dev` brings up both the backend (`:7878`) and the vite
dev server (`:5173`, HMR) in one command; Ctrl-C stops both. Then open
`http://127.0.0.1:5173` (vite proxies `/v1` — including the `/v1/stream` SSE feed —
to the backend; set `WIMCC_PROXY_TARGET` to point at another serve instance). To run them in separate terminals instead:
`just serve-dev` (backend) and `just webui-dev` (frontend).

**Node version** — Node 20 for the build (`webui/.nvmrc`). The untagged-Bash
tooling script (`webui/scripts/untagged-bash.ts`) needs Node 22+ for native type
stripping.

**dev DB regeneration** — after a migration change (current head in `migrations/`), run
`wimcc init-db` and re-ingest. Payload fields stored as JSON BLOBs
(`tool_call.tool_name`, `assistant_message.model`, …) are added without a schema
migration, so existing events won't have them until re-ingested.

## CI & releases

- **CI** (GitHub Actions) runs the full gate on every PR: `vitest` + the SPA
  build, then `cargo fmt --check`, `cargo clippy -- -D warnings`, and
  `cargo test` against the freshly built `webui/dist`.
- **Releases** are split across two workflows: release-please accumulates
  conventional commits on `main` into a release PR and, on merge, tags
  `vX.Y.Z`, generates the CHANGELOG, and creates the GitHub Release. dist
  (cargo-dist) then builds the 4 targets and the shell/Homebrew/npm
  installers, uploading everything to that same Release. Publishing to
  crates.io is a separate custom job: it gates on the embedded `webui/dist`
  (`scripts/check-crate-contents.sh`) and runs `cargo publish` independently
  of the dist upload. Versions in `Cargo.toml` and `webui/package.json` are
  bumped together — don't edit them by hand.

## Reference docs

- Full system specs: `docs/index.html` and `docs/00..05_*.html`
- Implementation notes (deviations, decisions, event-first redesign):
  `docs/implementation-notes.html`
- Project guidance for contributors: `CLAUDE.md`

