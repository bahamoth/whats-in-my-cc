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

```bash
cargo test
```

## Known limits in slice-1

- No redaction. Do not point at JSONL files that may contain secrets you're unwilling to expose to anything that can reach 127.0.0.1.
- No live tail. Re-run `ingest` to pick up newly appended JSONL lines (idempotent).
- `tool_call_to_result` edges appear as a self-loop (`from == to == tool_call`) with `attributes.merged = true` for matched calls; dangling tool_results get a regular call→result edge.
- `last-prompt`, `permission-mode`, `file-history-snapshot`, `thinking`, and non-hook `attachment` events are preserved as ObservedEvents but do not get their own graph nodes.

## Reference docs

- Spec (this slice): `docs/superpowers/specs/2026-05-19-witmcc-slice1-transcript-design.md`
- Plan: `docs/superpowers/plans/2026-05-19-witmcc-slice1-transcript.md`
- Full system docs: `docs/index.html` and `docs/00..06_*.html`
- Implementation notes (deviations & decisions): `docs/implementation-notes.html`
