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

### OTel Traces Receiver (slice-3)

`POST /otel/v1/traces` accepts OTLP/JSON traces. gzip-encoded request bodies are
decompressed automatically. Set your exporter to JSON:

```bash
export OTEL_EXPORTER_OTLP_TRACES_PROTOCOL=http/json
export OTEL_EXPORTER_OTLP_ENDPOINT=http://127.0.0.1:7878/otel
```

Manual smoke test:

```bash
curl -X POST http://127.0.0.1:7878/otel/v1/traces \
  -H 'Content-Type: application/json' \
  --data-binary @tests/fixtures/otel/single_span.json
```

Notes:
- traces signal only — metrics/logs are future slices.
- Spans without `session.id` are stored but excluded from `/v1/sessions`.
- No redaction yet — do not send spans containing secrets.

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

## Reference docs

- Spec (this slice): `docs/superpowers/specs/2026-05-19-witmcc-slice1-transcript-design.md`
- Plan: `docs/superpowers/plans/2026-05-19-witmcc-slice1-transcript.md`
- Full system docs: `docs/index.html` and `docs/00..06_*.html`
- Implementation notes (deviations & decisions): `docs/implementation-notes.html`
