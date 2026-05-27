# Slice-11 Design — VerificationRun Ingest

**Date:** 2026-05-27
**Branch (to be cut):** `slice11-verification-run` off the merge commit that lands slice-10a + this roadmap.
**Goal:** Emit `VerificationRun` rows for every observed verification event. Wire them onto the graph as `verification_run` nodes with `triggered_verification` (from tool_call) and `covers_diff_hunk` (to previously-introduced diff_hunks in the same session) edges. Expose them via Pull API.

This slice closes AC-3 ("file lineage → tool call → episode → verification") to the extent file → tool_call → verification is observable. The episode link lands in slice-12.

---

## 1. Motivation

`docs/06_mvp_execution_plan.html` AC-3 says:

> A diff hunk can be traced to tool call, episode, and verification run when observed.

Currently `repo_diff_hunk` rows link to tool calls (slice-10a follow-up), but there is no `VerificationRun` entity. Without it, the M5 Insight engine cannot fire `missing_verification` against anything — the rule is structurally inexpressible.

The two highest-value finding categories in M5 (`missing_verification`, `tool_failure`) both depend on having `VerificationRun` rows. Therefore the cheapest path to MVP-exit goes through this slice first.

Per the architecture spec for slice-10a, **commit SHAs are not used as verification markers**. The three legitimate sources of verification observation are:

1. **Bash test command results.** A subset of `Bash` tool calls whose command is a known test invocation produce verification runs whose `status` comes from the tool_result's exit code (`is_error` ↔ `failed`/`passed`).
2. **Hook verification commands.** PreToolUse / PostToolUse hook events that explicitly self-identify as verification (`hook_event_name == "PostToolUse"` and `hook_input.tool_name == "Bash"` and the matched command is on the allowlist) are also a verification run. This branch produces the same row from a different source — dedupe by `trigger_event_id`.
3. **OTel verification spans.** Spans carrying `attributes["verification.kind"]` are a verification run. **No real data captures this yet** — this branch is parser-version-gated and exists only so the spec freezes the right surface area now.

---

## 2. Scope

### In scope

- New table `verification_run` (sqlx migration `0005_verification_run.sql`).
- New `EventKind::VerificationRun` variant — wait, no: per architecture, verification_run is a **side-table referenced by tool_result events**, not a new ObservedEvent kind. The graph builder synthesises `verification_run` graph nodes the way it now synthesises `diff_hunk` nodes (`compute()` takes the side-table as input).
- New extractor `src/ingest/verification_run.rs` that walks `tool_result` ObservedEvents post-ingest and emits `VerificationRunRecord` rows.
- New `repo_verification_run` module mirroring `repo_diff_hunk` shape.
- Bash command allowlist `BashTestCommands::ALLOWLIST` — frozen `&[&str]` array of regex patterns. Locked by real-fixture invariant test.
- Hook branch in the same extractor, matching `hook_event_name == "PostToolUse"` + Bash + allowlist match.
- OTel branch: defensive code path, gated by `cfg(test)` until a real fixture exists. The branch lives in code so the public surface is correct, but emits no rows in production.
- Graph wiring: `compute()` signature extended to take `&[VerificationRunRow]`. New node kind `"verification_run"`. New edges:
  - `triggered_verification` (deterministic): `tool_call → verification_run`. Key: `(session_id, trigger_event_id)`.
  - `covers_diff_hunk` (deterministic, temporal): `verification_run → diff_hunk` for every diff_hunk in the same session whose `introduced_by_event_id` is from a `tool_result` strictly before the verification's `started_at`.
- Pull API endpoints:
  - `GET /v1/sessions/:id/verification-runs` — list runs in observed order.
  - `GET /v1/verification-runs/:id` — single run detail.
  - (Existing `/v1/sessions/:id/graph` automatically picks up new node/edge kinds.)
- WebUI: **no changes** beyond what falls out naturally from graph having new node kinds. The lane mapping returns `null` for `verification_run` (placeholder; UI redesign owns the lane). A negative-lock vitest case asserts the `null` so the redesign can detect when the surface is intentional vs accidental.

### Out of scope

- Wiring `verification_run` into a UI lane (UX redesign epic).
- Finding generation that depends on these rows (slice-14).
- OTel verification span real-fixture freezing (no captured span carries `verification.kind` yet).
- Fuzzy file-path matching for `covers_diff_hunk` (strict temporal precedence only; refinement is a slice-13 inferred edge if needed).
- Verification of partial coverage (a test run that only exercises one of three edited files): out of scope for MVP because we cannot observe coverage data here without running the test ourselves.

---

## 3. Real-data invariant

### Bash command allowlist

The allowlist is locked by a real-fixture invariant test. We need to find every real `Bash` tool call in the local transcripts whose command would qualify, and write the test to assert each one is matched by exactly one allowlist regex.

```rust
pub mod allowlist {
    pub const PATTERNS: &[(&str, &str)] = &[
        // (regex,                                                  kind)
        (r"^npm (run )?test(?:[: ].*)?$",                            "test_suite_js"),
        (r"^pnpm (run )?test(?:[: ].*)?$",                           "test_suite_js"),
        (r"^yarn (run )?test(?:[: ].*)?$",                           "test_suite_js"),
        (r"^vitest( run)?(?:[: ].*)?$",                              "test_suite_js"),
        (r"^jest(?:[: ].*)?$",                                       "test_suite_js"),
        (r"^mocha(?:[: ].*)?$",                                      "test_suite_js"),
        (r"^cargo (test|nextest)(?:[: ].*)?$",                       "test_suite_rust"),
        (r"^cargo check(?:[: ].*)?$",                                "build_check"),
        (r"^cargo build(?:[: ].*)?$",                                "build"),
        (r"^cargo clippy(?:[: ].*)?$",                               "lint"),
        (r"^cargo fmt(?: --check)?(?:[: ].*)?$",                     "format_check"),
        (r"^pytest(?:[: ].*)?$",                                     "test_suite_py"),
        (r"^python -m pytest(?:[: ].*)?$",                           "test_suite_py"),
        (r"^go test(?:[: ].*)?$",                                    "test_suite_go"),
        (r"^mvn test(?:[: ].*)?$",                                   "test_suite_java"),
        (r"^gradle test(?:[: ].*)?$",                                "test_suite_java"),
    ];
}
```

This list is closed. Adding a new pattern requires bumping the rule version (next slice that needs it). The test in `tests/verification_bash_allowlist.rs` enforces:

1. Every pattern compiles as a valid regex.
2. The list contains the patterns above (count + content), so additions/removals are explicit.
3. Each pattern matches the corresponding curated sample command (anchor test).
4. Patterns do **not** match commands that are not tests — anchored by `^…$` and a deny list of false-positive samples (`"npm install"`, `"cargo run"`, `"pytest --help"` should match because it's still pytest, but `"cargo doc"` should not).

### Real-fixture freezing

We currently have local transcripts at `~/.claude/projects/-Users-bahamoth-projects-whats-in-my-cc/*.jsonl`. The fixture freeze step:

1. Grep all `Bash` `tool_use` `input.command` strings across those transcripts.
2. For each command that matches **any** allowlist pattern, freeze the wrapping `tool_use` + paired `tool_result` line into `tests/fixtures/transcripts/real/verification_v01.jsonl`.
3. The invariant test in `tests/transcript_verification_bash.rs` deserialises the fixture, applies the extractor, and asserts the row count equals the fixture's known count.

If the real transcripts do not contain ≥1 Bash test command (possible in some user environments), the slice's invariant test falls back to a **single curated transcript fixture** — `tests/fixtures/transcripts/curated/verification_curated.jsonl` — containing a hand-rolled but shape-correct line. This fallback is marked in the test name (`extractor_locks_curated_fixture_when_real_unavailable`) so future maintainers know the test is not real-data anchored for that path. Per CLAUDE.md, the tradeoff is recorded in implementation-notes once slice-11 lands.

### Hook fixture

`tests/fixtures/hooks/post_tool_use_bash.json` — a frozen real hook payload for `PostToolUse` of a `Bash` tool whose command was on the allowlist. The hook stdin shape is locked by slice-4's existing fixtures; this slice extends them with one more example whose `hook_event_name` and matched command exercise the extractor.

### OTel branch

No fixture; the branch is exercised only by synthetic test data in `tests/verification_otel_synth.rs`. The test explicitly notes "synthetic — no real verification.kind span has been observed". The branch's existence is justified by the spec freeze: if the branch were absent and a future Claude Code version started emitting these spans, we would silently drop them.

---

## 4. Schema

### `verification_run` table

```sql
-- migrations/20260527120000_0005_verification_run.sql
CREATE TABLE IF NOT EXISTS verification_run (
    verification_run_id   TEXT PRIMARY KEY,         -- "vr_" + sha256(session_id||trigger_event_id||started_at)
    schema_version        TEXT NOT NULL DEFAULT 'verification_run.v1',
    session_id            TEXT NOT NULL,
    source                TEXT NOT NULL,            -- "bash" | "hook" | "otel"
    command               TEXT NOT NULL,            -- the matched command (redacted-after-storage per slice-18)
    command_kind          TEXT NOT NULL,            -- "test_suite_js" | "test_suite_rust" | ...
    trigger_event_id      TEXT NOT NULL,            -- observed_event.event_id that triggered this row
    trigger_tool_use_id   TEXT,                     -- nullable for the otel branch
    status                TEXT NOT NULL,            -- "passed" | "failed" | "unknown"
    started_at            TEXT NOT NULL,            -- ISO 8601 UTC
    ended_at              TEXT,                     -- ISO 8601 UTC; may be null if not derivable
    exit_code             INTEGER,                  -- bash branch only
    failure_summary       TEXT,                     -- first 512 bytes of stderr or otel status_message
    raw_event_id          TEXT NOT NULL,            -- FK-ish into raw_event
    parser_version        TEXT NOT NULL,
    created_at            TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_verification_run_session_started
    ON verification_run(session_id, started_at);

CREATE INDEX IF NOT EXISTS idx_verification_run_trigger
    ON verification_run(trigger_event_id);
```

### Why a new migration (not in-place)

Slice-10a edited migration 0003 in place; that was a recorded tradeoff (DEV-S10A-05). This slice adds a **new** numbered migration `0005`. We do not edit existing migrations again. Reason: slice-10a's in-place edit was justified because the dev DB was disposable at that point; once production-ish data starts accumulating (verification runs the user cares about), in-place edits become destructive.

### Graph node + edge kinds

`GraphNode.node_kind = "verification_run"`:

```json
{
  "node_id": "node_vr_<deterministic>",
  "node_kind": "verification_run",
  "session_id": "sess_…",
  "source_event_ids": ["ev_tool_result_…"],
  "payload": {
    "verification_run": { /* full row */ }
  }
}
```

Edges:

- `triggered_verification` — `tool_call → verification_run`. `attributes = { source: "bash" | "hook" | "otel" }`.
- `covers_diff_hunk` — `verification_run → diff_hunk` (one per matching diff_hunk). `attributes = { match: "temporal_session" }` indicating the only coverage signal we have.

### `compute()` signature change

```rust
pub fn compute(
    session_id: &str,
    events: &[ObservedEvent],
    hunks: &[DiffHunkRow],
    runs: &[VerificationRunRow],
) -> (Vec<GraphNode>, Vec<GraphEdge>)
```

The signature widening is consistent with slice-10a's pattern (DEV-S10A-11). Tests that synthesise a `compute()` input pass `&[]` for the new parameter unless they exercise verification rows.

---

## 5. Pull API surface

### `GET /v1/sessions/:id/verification-runs`

Response envelope (matches existing shape):

```json
{
  "data": [
    {
      "verification_run_id": "vr_…",
      "schema_version": "verification_run.v1",
      "session_id": "sess_…",
      "source": "bash",
      "command": "cargo test",
      "command_kind": "test_suite_rust",
      "trigger_event_id": "ev_…",
      "trigger_tool_use_id": "toolu_…",
      "status": "failed",
      "started_at": "2026-05-27T…",
      "ended_at": "2026-05-27T…",
      "exit_code": 101,
      "failure_summary": "test foo::bar failed",
      "covered_diff_hunk_ids": ["dh_…", "dh_…"]
    }
  ],
  "meta": { "schema_version": "response_envelope.v1", ... }
}
```

The `covered_diff_hunk_ids` array is **computed at response time** from the same temporal rule used for the graph edge. We do not store it as a column to avoid a denormalised invariant.

### `GET /v1/verification-runs/:id`

Same shape, single object in `data`.

### `GET /v1/sessions/:id/graph`

No new endpoint. Existing endpoint picks up the new node + edge kinds automatically because `compute()` returns them.

---

## 6. Provenance & parser_version

`verification_run.parser_version = "verification_run@v1"`. Bumped only when allowlist patterns or status derivation change. The parser_version is stamped on every row so a future change can be filtered out of analyses that depended on the old behaviour.

---

## 7. Failure modes

| Failure | Behaviour |
|---|---|
| Bash command matches an allowlist pattern but `tool_result` is absent (no pair) | Skip row; record nothing. Re-run will pick it up if the pair appears later. |
| Hook fires but the matched Bash tool_use_id is not (yet) in the DB | Skip row; reconciliation happens on next graph rebuild. |
| OTel span has `verification.kind` but no `trace_id` or `span_id` | Drop the row, log a `parser_error` finding candidate (slice-14 owns the surface; for now just a tracing warn). |
| Allowlist regex compile fails at startup | Process fails fast at boot. The startup test asserts every pattern compiles. |
| Duplicate `(session_id, trigger_event_id)` | `verification_run_id` is deterministic on those keys; INSERT OR REPLACE keeps the latest. |

---

## 8. Deviations index (slice-11)

| ID | Description |
|---|---|
| DEV-S11-01 | OTel verification branch exists in code but emits zero rows in production due to no real-data fixture. Listed here so a future reviewer does not delete it as dead code. |
| DEV-S11-02 | `covered_diff_hunk_ids` is computed at response time, not stored. Keeps the table denormalised-free; cost is one JOIN per request, acceptable given session-scoped queries. |
| DEV-S11-03 | Allowlist is a **closed** list of patterns. Adding patterns requires the slice that needs them; we do not accept user-configurable allowlists in MVP. Rationale: any pattern that's permanent in our codebase should be acceptance-tested against real fixtures, not configured at runtime. |
| DEV-S11-04 | `verification_run` is not a new `EventKind`. It is a side-table referenced by `tool_result` events. Reason: making it an EventKind would require an `observed_event` row with no corresponding raw_event line (synthetic), or a re-ingest of raw payload (waste). Side-table mirrors `diff_hunk`'s pattern from slice-10a. |
| DEV-S11-05 | `triggered_verification` edge from `tool_call` to `verification_run` is deterministic (key: trigger_event_id). It is not "inferred" — there is no ambiguity. Slice-13's inferred-edge framework therefore does not produce this edge. |
| DEV-S11-06 | When real allowlist-matching Bash commands are absent from local transcripts, fall back to a curated fixture. The fallback is explicit in test name and recorded in implementation-notes. This is the only synthetic anchor in slice-11. |

---

## 9. Commit plan summary

See the matching plan document `2026-05-27-witmcc-slice11-verification-run.md`. Five commits total:

1. `test(slice-11): red-locking tests for verification_run extractor + allowlist`
2. `feat(db): 0005_verification_run migration + repo_verification_run`
3. `feat(ingest): verification_run extractor — bash + hook + otel-synth`
4. `feat(graph): verification_run node + triggered_verification + covers_diff_hunk edges`
5. `feat(api): /v1/sessions/:id/verification-runs + /v1/verification-runs/:id`

Each commit leaves both `cargo test` and `vitest run` green. Smoke runs in a final ad-hoc step before the PR is opened.
