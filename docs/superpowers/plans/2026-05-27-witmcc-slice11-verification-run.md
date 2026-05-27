# Slice-11 Implementation Plan — VerificationRun Ingest

**Spec:** `docs/superpowers/specs/2026-05-27-witmcc-slice11-verification-run-design.md`
**Branch:** `slice11-verification-run`
**Strategy:** TDD red-first. Five phases, five commits. Every phase ends with `cargo test` + `vitest run` green; commit only after both pass.

---

## Phase 0 — Branch & baseline

| # | Task | Action | Verify |
|---|---|---|---|
| 0a | Cut branch | `git checkout main && git pull && git checkout -b slice11-verification-run` | `git status` clean |
| 0b | Baseline test counts | `cargo test 2>&1 \| tail -5` and `cd webui && npx vitest run 2>&1 \| tail -3` | Record numbers in a scratch note (PR description will quote them) |
| 0c | Confirm baseline rebuild latency for `aac68973` | `cargo run --release -- rebuild --session aac68973 2>&1 \| tail -3` (if a rebuild CLI exists, else `witmcc serve` + `curl …/graph?force_rebuild=1`) | Record. Slice-11 target: no more than +25 % latency. |

No commit.

---

## Phase 1 — Red-locking tests

### Task 1 — Allowlist invariant

**Files:**
- Create: `src/insight/verification_allowlist.rs`
- Create: `tests/verification_bash_allowlist.rs`

- [ ] **Step 1: Write the failing tests**

```rust
// tests/verification_bash_allowlist.rs
use witmcc::insight::verification_allowlist::{allowlist_patterns, classify};

#[test]
fn allowlist_has_expected_pattern_count() {
    // Locked count — adding/removing a pattern requires updating this number
    // AND providing a new sample command + a new no-match deny sample.
    assert_eq!(allowlist_patterns().len(), 16);
}

#[test]
fn every_pattern_compiles_as_regex() {
    for (re, _) in allowlist_patterns() {
        regex::Regex::new(re).unwrap_or_else(|_| panic!("invalid regex: {}", re));
    }
}

#[test]
fn classify_matches_curated_commands() {
    let samples: &[(&str, &str)] = &[
        ("npm test",                "test_suite_js"),
        ("npm run test",            "test_suite_js"),
        ("pnpm test",               "test_suite_js"),
        ("yarn test",               "test_suite_js"),
        ("vitest run",              "test_suite_js"),
        ("jest",                    "test_suite_js"),
        ("mocha",                   "test_suite_js"),
        ("cargo test",              "test_suite_rust"),
        ("cargo nextest run",       "test_suite_rust"),
        ("cargo check",             "build_check"),
        ("cargo build",             "build"),
        ("cargo clippy",            "lint"),
        ("cargo fmt --check",       "format_check"),
        ("pytest",                  "test_suite_py"),
        ("python -m pytest",        "test_suite_py"),
        ("go test ./...",           "test_suite_go"),
        ("mvn test",                "test_suite_java"),
        ("gradle test",             "test_suite_java"),
    ];
    for (cmd, want_kind) in samples {
        let got = classify(cmd);
        assert_eq!(
            got.as_deref(), Some(*want_kind),
            "command {:?} should classify as {:?}, got {:?}", cmd, want_kind, got
        );
    }
}

#[test]
fn classify_rejects_non_test_commands() {
    let deny: &[&str] = &[
        "npm install",
        "cargo run",
        "cargo doc",
        "git status",
        "ls -la",
        "echo cargo test",   // not anchored at start
        "cargo test && rm -rf /", // composite — out of scope; only the leading cmd classifies, but we choose to reject composites
    ];
    for cmd in deny {
        assert!(
            classify(cmd).is_none(),
            "command {:?} should not classify; got {:?}", cmd, classify(cmd)
        );
    }
}
```

- [ ] **Step 2: Verify it fails**

```bash
cargo test --test verification_bash_allowlist
```

Expected: compile error (module not found) — that is the red.

- [ ] **Step 3: Add module skeleton (compile only, no logic)**

```rust
// src/insight/verification_allowlist.rs
pub fn allowlist_patterns() -> &'static [(&'static str, &'static str)] {
    &[]
}
pub fn classify(_cmd: &str) -> Option<&'static str> {
    None
}
```

Also register the module:

```rust
// src/insight/mod.rs (create if not present)
pub mod verification_allowlist;
```

Add `pub mod insight;` to `src/lib.rs` (after the existing `pub mod` lines, alphabetically).

- [ ] **Step 4: Re-run test, expect logical failures (not compile)**

```bash
cargo test --test verification_bash_allowlist
```

Expected: `allowlist_has_expected_pattern_count` fails with `assertion `left == right` failed: 0 vs 16`. Good — implementation pending.

No commit yet (the module body lands in Phase 2).

### Task 2 — Real-fixture freeze

**Files:**
- Create: `tests/fixtures/transcripts/real/verification_v01.jsonl`
- (Optional fallback) Create: `tests/fixtures/transcripts/curated/verification_curated.jsonl`

- [ ] **Step 1: Identify candidate lines in local transcripts**

Run a one-liner to find every `Bash` tool_use whose command would match the allowlist (use a coarse pre-filter):

```bash
rg -l '"name":\s*"Bash"' ~/.claude/projects/-Users-bahamoth-projects-whats-in-my-cc/ \
  | head -5
```

Then for each candidate, inspect:

```bash
jq -c 'select(.message.content[]?.type == "tool_use" and .message.content[]?.name == "Bash")
       | .message.content[] | select(.type == "tool_use" and .name == "Bash") | .input.command' \
  ~/.claude/projects/-Users-bahamoth-projects-whats-in-my-cc/<file>.jsonl \
  | sort -u
```

- [ ] **Step 2: Freeze at minimum 3 (tool_use, tool_result) pairs**

Pick (a) a passing test, (b) a failing test, (c) a build/check command. Copy the corresponding JSONL lines verbatim into `verification_v01.jsonl`. Each fixture line is one transcript record; the file contains six lines (three pairs).

- [ ] **Step 3: If fewer than three pairs exist locally, create the curated fallback**

`tests/fixtures/transcripts/curated/verification_curated.jsonl` — three hand-rolled lines matching the existing transcript shape with command set to `cargo test`. Mark this case in implementation-notes (DEV-S11-06).

### Task 3 — Extractor invariant (red)

**Files:**
- Create: `tests/transcript_verification_bash.rs`

- [ ] **Step 1: Write failing test against fixture**

```rust
// tests/transcript_verification_bash.rs
use witmcc::ingest::verification_run::extract_verification_runs;
use witmcc::model::observed::ObservedEvent;

fn load_fixture(path: &str) -> Vec<ObservedEvent> {
    // Re-use existing fixture loader from tests/parser.rs; if not available,
    // open the file, deserialise each line via the transcript parser.
    unimplemented!()
}

#[test]
fn extracts_verification_runs_from_real_bash_fixture() {
    let evs = load_fixture("tests/fixtures/transcripts/real/verification_v01.jsonl");
    let runs = extract_verification_runs(&evs);
    // Fixture has three Bash test pairs: one passing, one failing, one build.
    assert_eq!(runs.len(), 3, "expected 3 runs, got {}", runs.len());

    let passed = runs.iter().filter(|r| r.status == "passed").count();
    let failed = runs.iter().filter(|r| r.status == "failed").count();
    assert!(passed >= 1, "at least one passing run expected");
    assert!(failed >= 1, "at least one failing run expected");

    for r in &runs {
        assert!(!r.session_id.is_empty());
        assert!(!r.trigger_event_id.is_empty());
        assert!(!r.command.is_empty());
        assert!(!r.command_kind.is_empty());
        assert!(["bash", "hook", "otel"].contains(&r.source.as_str()));
    }
}

#[test]
fn produces_no_runs_for_non_test_bash() {
    use serde_json::json;
    let evs = vec![
        // Synthetic tool_use + tool_result for `git status` — must produce no runs.
        // Build via the same helper used in tests/mapping.rs.
        // ...
    ];
    let runs = extract_verification_runs(&evs);
    assert!(runs.is_empty());
}

#[test]
fn deduplicates_by_trigger_event_id() {
    // Two passes over the same events should produce identical row IDs (deterministic).
    // ...
}
```

(Step body uses helpers re-exported from existing tests; if the helpers need to be promoted to a shared module, do that in this task — keep the change small, file `tests/common/mod.rs`.)

- [ ] **Step 2: Run, expect compile error (module + function missing)**

```bash
cargo test --test transcript_verification_bash
```

Expected: compile error referencing `extract_verification_runs` and `ingest::verification_run`.

- [ ] **Step 3: Stub the module**

```rust
// src/ingest/verification_run.rs
use crate::model::observed::ObservedEvent;

#[derive(Debug, Clone)]
pub struct VerificationRunRecord {
    pub verification_run_id: String,
    pub schema_version: &'static str,
    pub session_id: String,
    pub source: String,
    pub command: String,
    pub command_kind: String,
    pub trigger_event_id: String,
    pub trigger_tool_use_id: Option<String>,
    pub status: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub exit_code: Option<i32>,
    pub failure_summary: Option<String>,
    pub raw_event_id: String,
    pub parser_version: &'static str,
}

pub fn extract_verification_runs(_evs: &[ObservedEvent]) -> Vec<VerificationRunRecord> {
    Vec::new()
}
```

Register `pub mod verification_run;` in `src/ingest/mod.rs`.

- [ ] **Step 4: Re-run, expect assertion failures (compile passes, logic missing)**

```bash
cargo test --test transcript_verification_bash
```

Expected: `extracts_verification_runs_from_real_bash_fixture` fails (3 expected, 0 got). Good red.

### Task 4 — Schema-shape invariant (red)

**Files:**
- Create: `tests/migration_verification_run_schema.rs`

- [ ] **Step 1: Write test against fresh in-memory DB**

```rust
use sqlx::SqlitePool;

#[tokio::test]
async fn migration_creates_verification_run_table_with_expected_columns() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let cols: Vec<(String, String, i32, i32)> = sqlx::query_as(
        "SELECT name, type, notnull, pk FROM pragma_table_info('verification_run')"
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    let names: Vec<String> = cols.iter().map(|c| c.0.clone()).collect();
    let expected = vec![
        "verification_run_id","schema_version","session_id","source","command",
        "command_kind","trigger_event_id","trigger_tool_use_id","status",
        "started_at","ended_at","exit_code","failure_summary",
        "raw_event_id","parser_version","created_at",
    ];
    for c in expected {
        assert!(names.contains(&c.to_string()), "missing column {}", c);
    }

    // verification_run_id is PK
    assert!(cols.iter().any(|c| c.0 == "verification_run_id" && c.3 == 1));
}
```

- [ ] **Step 2: Run; expect failure (migration not present)**

```bash
cargo test --test migration_verification_run_schema
```

Expected: panic on `pragma_table_info` returning empty rows.

### Task 5 — API endpoint shape (red)

**Files:**
- Create: `tests/api_verification_runs.rs`

- [ ] **Step 1: Write failing test**

```rust
use axum_test::TestServer;
use witmcc::api::build_router;

#[tokio::test]
async fn endpoint_returns_runs_for_session() {
    let pool = test_pool_with_seeded_runs().await; // helper: insert 2 verification_run rows for session "sess_t1"
    let router = build_router(pool.clone());
    let server = TestServer::new(router).unwrap();

    let resp = server.get("/v1/sessions/sess_t1/verification-runs").await;
    resp.assert_status_ok();
    let body: serde_json::Value = resp.json();
    assert_eq!(body["data"].as_array().unwrap().len(), 2);
    let first = &body["data"][0];
    assert!(first["verification_run_id"].is_string());
    assert!(first["covered_diff_hunk_ids"].is_array());
}

#[tokio::test]
async fn endpoint_returns_empty_for_unknown_session() {
    let pool = test_pool_empty().await;
    let router = build_router(pool);
    let server = TestServer::new(router).unwrap();
    let resp = server.get("/v1/sessions/unknown/verification-runs").await;
    resp.assert_status_ok();
    let body: serde_json::Value = resp.json();
    assert_eq!(body["data"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn single_run_detail_endpoint() {
    let pool = test_pool_with_seeded_runs().await;
    let router = build_router(pool);
    let server = TestServer::new(router).unwrap();
    let resp = server.get("/v1/verification-runs/vr_t1_001").await;
    resp.assert_status_ok();
    let body: serde_json::Value = resp.json();
    assert_eq!(body["data"]["verification_run_id"], "vr_t1_001");
}
```

- [ ] **Step 2: Run; expect 404 because routes not registered**

```bash
cargo test --test api_verification_runs
```

### Task 6 — Graph integration (red)

**Files:**
- Create: `tests/graph_verification_run_node.rs`

- [ ] **Step 1: Write failing test**

```rust
use witmcc::graph::build::compute;
use witmcc::db::repo_verification_run::VerificationRunRow;

#[test]
fn compute_emits_verification_run_node_and_triggered_edge() {
    let session = "sess_test";
    let evs = build_minimal_session_with_bash_tool_call(session); // helper
    let hunks = vec![];
    let runs = vec![VerificationRunRow {
        verification_run_id: "vr_x".into(),
        session_id: session.into(),
        source: "bash".into(),
        command: "cargo test".into(),
        command_kind: "test_suite_rust".into(),
        trigger_event_id: evs[1].event_id.clone(),  // tool_result event
        trigger_tool_use_id: Some("toolu_x".into()),
        status: "passed".into(),
        started_at: "2026-05-27T10:00:00Z".into(),
        ended_at: Some("2026-05-27T10:00:05Z".into()),
        exit_code: Some(0),
        failure_summary: None,
        raw_event_id: "raw_x".into(),
        schema_version: "verification_run.v1".into(),
        parser_version: "verification_run@v1".into(),
    }];

    let (nodes, edges) = compute(session, &evs, &hunks, &runs);

    assert!(nodes.iter().any(|n| n.node_kind == "verification_run"));
    assert!(edges.iter().any(|e| e.edge_kind == "triggered_verification"));
}

#[test]
fn covers_diff_hunk_edge_links_temporal_precedence() {
    // Build a session with: tool_call (Edit) → tool_result(Edit, produces diff_hunk)
    //                       → tool_call (Bash) → tool_result(Bash, verification passed).
    // Assert the resulting graph has a covers_diff_hunk edge from verification_run to diff_hunk.
    // ...
}
```

- [ ] **Step 2: Run; expect compile error on `compute()` signature**

```bash
cargo test --test graph_verification_run_node
```

Expected: `compute` takes 3 args, got 4.

### Phase-1 commit

**Commit 1:** `test(slice-11): red-locking tests for verification_run extractor, schema, graph, API`

```bash
git add src/insight/verification_allowlist.rs src/insight/mod.rs src/lib.rs \
        src/ingest/verification_run.rs src/ingest/mod.rs \
        tests/verification_bash_allowlist.rs tests/transcript_verification_bash.rs \
        tests/migration_verification_run_schema.rs tests/api_verification_runs.rs \
        tests/graph_verification_run_node.rs \
        tests/fixtures/transcripts/real/verification_v01.jsonl
git commit -m "test(slice-11): red-locking tests for verification_run pipeline"
```

Expected state at this commit: red on extractor body, schema migration, API routes, graph compute() signature, allowlist body. Cargo build still passes (stubs compile). Vitest is unchanged.

> **Self-check before commit 1:** real-data fixture exists (or fallback recorded). No UI change. Allowlist count is locked. All five failing tests are explicit reds, not silent stubs.

---

## Phase 2 — DB migration + repo

| # | Task | Verify |
|---|---|---|
| 7 | Author `migrations/20260527120000_0005_verification_run.sql` per spec §4 | `cargo test --test migration_verification_run_schema` turns green |
| 8 | Create `src/db/repo_verification_run.rs` with `insert(row) / list_session(pool, sid) / get(id) -> Option<Row>` | Add a small repo-level test `tests/repo_verification_run.rs` asserting roundtrip |
| 9 | Wire `repo_verification_run` into `src/db/mod.rs` | `cargo build` clean |

**Commit 2:** `feat(db): 0005_verification_run migration + repo_verification_run`

```bash
git add migrations/20260527120000_0005_verification_run.sql \
        src/db/repo_verification_run.rs src/db/mod.rs \
        tests/repo_verification_run.rs
git commit -m "feat(db): 0005_verification_run migration + repo_verification_run"
```

---

## Phase 3 — Extractor body

| # | Task | Verify |
|---|---|---|
| 10 | Fill out `extract_verification_runs` for the Bash branch — walk events, when `kind == ToolCall && tool_name == "Bash"`, match command via `classify`, find paired `ToolResult` by `tool_use_id`, derive `status` from `is_error`, emit row | `tests/transcript_verification_bash.rs` Bash assertions green |
| 11 | Fill out hook branch — walk `EventKind::HookEvent` with `subkind == "PostToolUse"`, read `hook_input.tool_name` + matched command from payload, emit row referencing the matched Bash tool_use_id. Dedupe by trigger_event_id — if a Bash row already exists, hook row is dropped. | Hook fixture test green |
| 12 | OTel branch (synthetic-only) — gate behind a feature module that requires `attributes["verification.kind"]`. Add `tests/verification_otel_synth.rs` to cover. | Synth test green |
| 13 | Fill out `allowlist_patterns()` and `classify()` per spec §3 | All 4 allowlist tests green |

**Commit 3:** `feat(ingest): verification_run extractor — bash + hook + otel-synth`

```bash
git add src/ingest/verification_run.rs src/insight/verification_allowlist.rs \
        tests/verification_otel_synth.rs
git commit -m "feat(ingest): verification_run extractor (bash, hook, otel-synth)"
```

---

## Phase 4 — Graph wiring

| # | Task | Verify |
|---|---|---|
| 14 | Widen `compute()` signature to take `&[VerificationRunRow]`. Update all callers (`rebuild_session` + every test that calls `compute()` directly — `tests/determinism.rs`, `tests/graph_build.rs`, `tests/graph_diff_hunk_node.rs`) to pass `&[]`. | `cargo build --tests` clean |
| 15 | In `compute()`, after the existing diff_hunk node loop, add a verification_run loop that materialises one node per run + a `triggered_verification` edge from the tool_call node found via `tool_call_node[trigger_tool_use_id]` (slice-1 maps it). | First test in `graph_verification_run_node.rs` green |
| 16 | Implement `covers_diff_hunk` edge generation — for each run, walk `hunks` and emit an edge for every hunk whose `introduced_by_event_id`'s observed_at < run's `started_at`. | Second test green |
| 17 | Update `rebuild_session` to read runs from the DB and pass them to `compute` | Run smoke check: rebuild aac68973 (after running ingestion that produces runs) and confirm new node + edge counts |

**Commit 4:** `feat(graph): verification_run node + triggered_verification + covers_diff_hunk edges`

```bash
git add src/graph/build.rs src/ingest/store.rs \
        tests/determinism.rs tests/graph_build.rs tests/graph_diff_hunk_node.rs \
        tests/graph_verification_run_node.rs
git commit -m "feat(graph): verification_run node + triggered/covers edges"
```

---

## Phase 5 — Pull API + WebUI placeholder

| # | Task | Verify |
|---|---|---|
| 18 | Add handler `list_verification_runs(State, Path) -> Json<Envelope<Vec<VerificationRunResponse>>>` in `src/api/routes.rs`. Compute `covered_diff_hunk_ids` per row by re-querying hunks in the same session and filtering by temporal rule. | First API test green |
| 19 | Add handler `verification_run_detail(State, Path)` returning single row + covered diff hunks | Detail test green |
| 20 | Register routes in `src/api/mod.rs` | Empty-session test green |
| 21 | WebUI lane mapping negative lock — extend `webui/src/api/__tests__/laneMapping.test.ts` with a case asserting `verification_run` returns `null` (placeholder pending UX redesign) | `npx vitest run` green |
| 22 | Update `webui/src/components/Timeline.tsx` if any guard rejects an unknown `node_kind` (just allow it through; the lane will be `null` and the UI ignores it). Verify no console warning. | Manual: open browser smoke |

**Commit 5:** `feat(api): /v1/sessions/:id/verification-runs + /v1/verification-runs/:id`

```bash
git add src/api/routes.rs src/api/mod.rs src/api/dto.rs \
        webui/src/api/__tests__/laneMapping.test.ts
git commit -m "feat(api): verification-runs endpoints + webui lane negative lock"
```

---

## Phase 6 — Smoke + verification

```
Smoke — slice-11

[ ] witmcc init-db (after deleting old .witmcc.sqlite*)
[ ] witmcc ingest /Users/bahamoth/.claude/projects/.../aac68973*.jsonl
[ ] witmcc serve --port 4337 &
[ ] curl -s http://127.0.0.1:4337/v1/sessions/aac68973/verification-runs | jq 'length'
    # Expected: ≥ 1 if aac68973 contains any allowlisted Bash command. Record the number.
[ ] curl -s http://127.0.0.1:4337/v1/sessions/aac68973/verification-runs | jq '.data[0]'
    # Verify covered_diff_hunk_ids is a JSON array (may be empty)
[ ] curl -s http://127.0.0.1:4337/v1/sessions/aac68973/graph | \
      jq '.data.nodes | map(select(.node_kind == "verification_run")) | length'
    # Should equal the count above
[ ] curl -s http://127.0.0.1:4337/v1/sessions/aac68973/graph | \
      jq '.data.edges | map(select(.edge_kind == "triggered_verification")) | length'
    # Should equal the count above
[ ] curl -s http://127.0.0.1:4337/v1/sessions/aac68973/graph | \
      jq '.data.edges | map(select(.edge_kind == "covers_diff_hunk")) | length'
    # Non-zero if any verification run started after at least one diff_hunk
[ ] Browser smoke (claude-in-chrome MCP if available):
    - Open http://127.0.0.1:4337/sessions/aac68973
    - Confirm Timeline lanes render without warnings
    - Confirm console clean (no “unknown node_kind” warnings)
```

Record the smoke output in the merge commit body. If the real transcript produces zero verification runs (no allowlisted Bash commands), record that fact explicitly and note that the curated fixture is the only test-time evidence.

```
Verification — slice-11

- cargo test count: baseline 189 → expected ≥ 189 + 14 (4 allowlist, 3 transcript, 1 schema, 3 api, 2 graph, 1 otel-synth)
- vitest run count: baseline 68 → expected ≥ 68 + 1 (laneMapping negative lock)
- aac68973 rebuild latency: baseline T → must be ≤ 1.25 × T
- aac68973 node count: baseline 1478 → +N (N = verification run count)
- aac68973 edge count: baseline 3085 → +M (M = triggered + covers count)
```

---

## Phase 7 — PR

- [ ] Push branch: `git push -u origin slice11-verification-run`
- [ ] Open PR using the §4.4 template from the roadmap. Title: `feat(slice-11): VerificationRun ingest — bash + hook + otel-synth`.
- [ ] Self-review: re-read spec §2 In-scope items, confirm every bullet has a corresponding test in the inventory.
- [ ] Add implementation-notes entry: open `docs/implementation-notes.html`, add an `Overview (slice-11)` section + `Intentional Deviations (slice-11)` listing DEV-S11-01..06 (see spec §8). Mention real-data anchoring status (real fixture used vs. curated fallback).
- [ ] Update CLAUDE.md status block: change `slice-1~10a 완료` line to `slice-1~11 완료`.
