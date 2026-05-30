# tool_failure Reframe — Implementation Plan (Slice 3 of insight-surface-redesign)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stop the `tool_failure` extractor from lumping internal agent auto-retries (e.g. `StructuredOutput` schema-retry cycles — 1941 of ~1953 "high" findings in session `653ea169`) and benign non-zero exits (`grep` no-match, `Read` file-not-found) into the same "high" severity bucket as genuine user-visible failures. Classify every fired `tool_failure` into one of three **failure classes** — `user_visible` | `internal_retry` | `benign_nonzero_exit` — persist that class on the finding, and demote the two noise classes to a new `info` severity so they never enter a `severity=high` headline. Add a count + drill query/DTO/route so the surface can ask for "user-visible tool failures" only. Unblocks the redesign's Q1 (efficiency) and Q2 (cost/token-waste attribution) — the 도구실패(사용자) card in §5.

**Architecture & key design decision (spec §6.3, P1/P2/P4 + CLAUDE.md evidence-linked + no-annotation):**

The candidate type carries `category: &'static str` and `severity: &'static str` (see `src/insight/types.rs`). The extractor already fires per distinct failed `tool_use_id` and projects an `evidence_projection` JSON object. We **keep every failure as a finding (option c — keep + tag), and additionally demote** the two noise classes to `info` severity. We do **not** drop internal retries (option a) because that destroys evidence the drill-down needs (CLAUDE.md: *Evidence-linked — Finding을 evidence_refs 없이 만들지 않는다*; deleting the rows hides the data the user must be able to inspect). We do **not** rely on severity demotion alone (option b) because the API/UI needs an explicit, queryable tag to render the "user-visible only" count and to keep internal noise out of any headline regardless of how severity is later interpreted. So:

1. A pure classifier `classify_failure(tool_name, error_excerpt) -> FailureClass` is unit-tested.
2. The class is written **two ways** for redundancy + queryability: (a) a new additive `subkind` column on `finding` (mirrors slice-13's additive columns on `graph_edge`; no data loss, INSERT-OR-REPLACE idempotent) and (b) `evidence_projection.failure_class` (source-preserving). The headline-driving `severity` becomes `info` for `internal_retry` / `benign_nonzero_exit`, stays `high` for `user_visible`.
3. The classification rule (precise, per spec §6.3 "Stop benign non-zero exits"): a failure is `internal_retry` when `tool_name == "StructuredOutput"`; `benign_nonzero_exit` when the error excerpt matches a small benign-pattern denylist (`grep` no-match, `Read` file-not-found / no-such-file); otherwise `user_visible`. The denylist is deliberately tiny and evidence-anchored, not "only Bash/Edit/Write count" (that alternative would silently drop real MCP / Task / browser failures the user cares about — see §6.3 "~28: Bash/Read/browser/Edit").
4. A new repo query `count_class` + DTO field + `?failure_class=` filter so the surface can request user-visible-only.

**Tech Stack:** Rust (sqlx + SQLite, serde_json), axum (Pull API). Frontend: React + TypeScript + @tanstack/react-query. Tests: `cargo test`, `npx vitest run`, `npx tsc -b`.

**Real-data anchoring note:** No frozen real fixture in `tests/fixtures/transcripts/real/` contains a `StructuredOutput` failure or any `tool_result` with `is_error=true` (verified: `grep -rl 'StructuredOutput' tests/fixtures` → none; `grep -rl '"is_error":true' tests/fixtures/transcripts` → none). Per CLAUDE.md real-data anchoring (option **b**: invariant assertion on a frozen payload) the *shape* of the `tool_result` payload (`{"tool_result":{"is_error":bool,"content":str,"tool_use_id":str}}`) is already locked by the existing `extractor_tool_failure.rs` comment referencing fixture `aac68973-729e-4014-a02b-28a556f5ff29`. The classifier's *behaviour* on `StructuredOutput` / benign excerpts is therefore tested with **synthetic** `ObservedEvent` fixtures (the same synthetic-view pattern the existing extractor tests already use) — this is explicitly the spec-sanctioned fallback (§9: "if none have StructuredOutput failures, a synthetic ObservedEvent fixture is acceptable"). This is flagged as a single-source synthetic assumption, not generalized.

**Out of scope for this plan (later slices):** the verification-detection rewrite (slice 2, §6.2), the episode classifier drift fix (slice 4, §6.4), cost 추정 (slice 5, §6.5), the 도구실패(사용자) KpiStrip card UI render + provenance badge (frontend surface slice). This slice delivers the classifier + persisted class + filtered count endpoint + the frontend type/query plumbing the card will consume — testable on its own.

---

## File structure

| File | Responsibility | Create/Modify |
|---|---|---|
| `migrations/20260603180000_0015_finding_subkind.sql` | additive `subkind` column + index on `finding` | Create |
| `src/insight/extractors/tool_failure.rs` | `FailureClass` enum + `classify_failure` + wire into `build_candidate`; add `failure_class` to projection; per-class severity | Modify |
| `src/insight/types.rs` | `FindingCandidate.subkind: Option<&'static str>` | Modify |
| `src/insight/pipeline.rs` | propagate `subkind` into `FindingRow` (both L1 + L2 build paths) | Modify |
| `src/db/repo_finding.rs` | `FindingRow.subkind`; insert/select include it; `ListFilter.subkind`; `count_class` helper | Modify |
| `src/api/dto.rs` | `FindingDto.subkind`; `ToolFailureSummaryDto` | Modify |
| `src/api/routes.rs` | `finding_row_to_dto` maps subkind; `FindingsQuery.subkind`; `session_tool_failures` handler | Modify |
| `src/api/mod.rs` | register `/v1/sessions/:id/tool-failures` route | Modify |
| `webui/src/api/types.ts` | `FindingDto.subkind`; `ToolFailureSummaryDto` | Modify |
| `webui/src/api/client.ts` | `getToolFailureSummary` | Modify |
| `webui/src/lib/queries.ts` | `sessionKeys.toolFailures` + `useToolFailureSummaryQuery` | Modify |
| `tests/extractor_tool_failure.rs` | classifier + per-class severity/subkind tests | Modify |
| `tests/migration_finding_schema.rs` | assert `subkind` column exists | Modify |
| `tests/api_tool_failures.rs` | endpoint contract test | Create |
| `webui/src/api/__tests__/client.endpoints.test.ts` | `getToolFailureSummary` case | Modify |

---

## Task 1: Migration — additive `subkind` column on `finding`

**Files:**
- Create: `migrations/20260603180000_0015_finding_subkind.sql`

- [ ] **Step 1: Write the migration**

Confirm the next migration number first: `ls migrations/ | sort | tail -3` (expected highest is `..._0014_usage_facet.sql`; slice-1 already added 0014). Then create the file:

```sql
-- Slice insight-surface-redesign #3 (tool_failure reframe, spec §6.3):
-- additive `subkind` column classifying a finding's sub-type. For tool_failure
-- this carries the failure class:
--   'user_visible'        — a genuine user-facing failure (severity high)
--   'internal_retry'      — internal agent auto-retry, e.g. StructuredOutput
--                           schema-retry cycles (severity info, never headlined)
--   'benign_nonzero_exit' — grep no-match / Read file-not-found (severity info)
-- NULL for findings of other categories that do not classify (back-compat).
-- Additive only — no data loss; mirrors slice-13 additive columns on graph_edge.

ALTER TABLE finding ADD COLUMN subkind TEXT;

CREATE INDEX IF NOT EXISTS idx_finding_subkind_session
    ON finding(session_id, category, subkind);
```

- [ ] **Step 2: Verify schema applies**

Run: `cargo run --bin witmcc -- init-db 2>&1 | tail -5`
Expected: no migration error; the sqlx migration hash set updates.

- [ ] **Step 3: Commit**

```bash
git add migrations/20260603180000_0015_finding_subkind.sql
git commit -m "feat(finding): migration 0015 — additive subkind column for tool_failure class"
```

---

## Task 2: `FailureClass` + pure `classify_failure` (RED → GREEN)

**Files:**
- Modify: `src/insight/extractors/tool_failure.rs`
- Modify: `tests/extractor_tool_failure.rs`

- [ ] **Step 1: Write the failing classifier test**

Append to `tests/extractor_tool_failure.rs` (it already imports `ToolFailure`, `InsightExtractor`, `SessionInsightView`, `Actor/EventKind/ObservedEvent` and defines `tool_call_ev`, `tool_result_ev`, `view_from_events`). First add the new import line near the top, beside the existing `use witmcc::insight::extractors::tool_failure::ToolFailure;`:

```rust
use witmcc::insight::extractors::tool_failure::{classify_failure, FailureClass};
```

Then append these tests (synthetic — see plan's real-data anchoring note):

```rust
/// StructuredOutput failures are internal agent auto-retries (spec §6.3) —
/// classified internal_retry regardless of the error text.
#[test]
fn classifies_structured_output_as_internal_retry() {
    assert_eq!(
        classify_failure("StructuredOutput", "schema validation failed: missing key"),
        FailureClass::InternalRetry
    );
}

/// grep no-match (exit 1) and Read file-not-found are benign non-zero exits,
/// not user-visible failures (spec §6.3 "Stop benign non-zero exits").
#[test]
fn classifies_benign_nonzero_exits() {
    assert_eq!(
        classify_failure("Bash", "grep: no matches found"),
        FailureClass::BenignNonzeroExit
    );
    assert_eq!(
        classify_failure("Read", "File does not exist: /tmp/missing.rs"),
        FailureClass::BenignNonzeroExit
    );
    assert_eq!(
        classify_failure("Read", "<tool_use_error>File does not exist.</tool_use_error>"),
        FailureClass::BenignNonzeroExit
    );
}

/// A real failing Bash build / Edit failure stays user_visible.
#[test]
fn classifies_real_failures_as_user_visible() {
    assert_eq!(
        classify_failure("Bash", "error[E0599]: no method named `foo`"),
        FailureClass::UserVisible
    );
    assert_eq!(
        classify_failure("Edit", "String to replace not found in file."),
        FailureClass::UserVisible
    );
    // unknown tool, ordinary error → user_visible (conservative; we surface it)
    assert_eq!(
        classify_failure("mcp__server__do_thing", "connection refused"),
        FailureClass::UserVisible
    );
}

/// FailureClass exposes its persisted string + severity mapping.
#[test]
fn failure_class_as_str_and_severity() {
    assert_eq!(FailureClass::UserVisible.as_str(), "user_visible");
    assert_eq!(FailureClass::InternalRetry.as_str(), "internal_retry");
    assert_eq!(FailureClass::BenignNonzeroExit.as_str(), "benign_nonzero_exit");
    assert_eq!(FailureClass::UserVisible.severity(), "high");
    assert_eq!(FailureClass::InternalRetry.severity(), "info");
    assert_eq!(FailureClass::BenignNonzeroExit.severity(), "info");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test extractor_tool_failure 2>&1 | tail -20`
Expected: FAIL to **compile** — `classify_failure` / `FailureClass` do not exist yet.

- [ ] **Step 3: Implement `FailureClass` + `classify_failure`**

In `src/insight/extractors/tool_failure.rs`, add below the existing `const RETRY_WINDOW: usize = 5;` line:

```rust
/// Tools whose failures are internal agent auto-retries, not user-visible
/// failures (spec §6.3). `StructuredOutput` is the workflow-subagent schema
/// tool that produces the 1941/1953 retry-cycle noise in session 653ea169.
const INTERNAL_RETRY_TOOLS: &[&str] = &["StructuredOutput"];

/// Substrings (lower-cased compare) that mark a benign non-zero exit: a tool
/// "failure" the user does not care about (grep no-match exit 1, Read of a
/// missing file). Kept deliberately tiny + evidence-anchored, not a blanket
/// "only Bash/Edit/Write count" rule (that would drop real MCP/Task/browser
/// failures, which §6.3 lists among the ~28 user-visible ones).
const BENIGN_EXIT_MARKERS: &[&str] = &[
    "no matches found",     // grep / ripgrep exit 1
    "file does not exist",  // Read tool not-found
    "no such file or directory",
];

/// The class a fired tool_failure falls into. Drives both the persisted
/// `subkind` and the finding `severity` (so internal noise never headlines).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureClass {
    /// Genuine user-facing failure — headline-eligible (severity high).
    UserVisible,
    /// Internal agent auto-retry (e.g. StructuredOutput) — severity info.
    InternalRetry,
    /// Benign non-zero exit (grep no-match, Read not-found) — severity info.
    BenignNonzeroExit,
}

impl FailureClass {
    /// Stable string persisted in `finding.subkind` + evidence_projection.
    pub fn as_str(self) -> &'static str {
        match self {
            FailureClass::UserVisible => "user_visible",
            FailureClass::InternalRetry => "internal_retry",
            FailureClass::BenignNonzeroExit => "benign_nonzero_exit",
        }
    }

    /// Severity for the finding. Only `user_visible` is `high`; the two noise
    /// classes are `info` so a `severity=high` headline never lumps them.
    pub fn severity(self) -> &'static str {
        match self {
            FailureClass::UserVisible => "high",
            FailureClass::InternalRetry | FailureClass::BenignNonzeroExit => "info",
        }
    }
}

/// Classify a fired tool_failure by tool name + error excerpt (spec §6.3).
/// Precedence: internal-retry tool first, then benign-exit markers, else
/// user_visible (conservative — an unrecognised failure is surfaced).
pub fn classify_failure(tool_name: &str, error_excerpt: &str) -> FailureClass {
    if INTERNAL_RETRY_TOOLS.contains(&tool_name) {
        return FailureClass::InternalRetry;
    }
    let lc = error_excerpt.to_ascii_lowercase();
    if BENIGN_EXIT_MARKERS.iter().any(|m| lc.contains(m)) {
        return FailureClass::BenignNonzeroExit;
    }
    FailureClass::UserVisible
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test extractor_tool_failure 2>&1 | tail -20`
Expected: the four new classifier tests PASS. (The pre-existing extractor tests still pass — `build_candidate` is wired in Task 3 next; nothing existing references the new symbols yet.)

- [ ] **Step 5: Commit**

```bash
git add src/insight/extractors/tool_failure.rs tests/extractor_tool_failure.rs
git commit -m "feat(tool_failure): FailureClass + classify_failure (StructuredOutput / benign exits)"
```

---

## Task 3: Wire class into candidate (severity + subkind + projection)

**Files:**
- Modify: `src/insight/types.rs` (add `subkind` to `FindingCandidate`)
- Modify: `src/insight/extractors/tool_failure.rs` (`build_candidate` uses the class)
- Modify: `tests/extractor_tool_failure.rs` (assert per-class severity + subkind on candidates)

- [ ] **Step 1: Write the failing candidate-level test**

Append to `tests/extractor_tool_failure.rs`. These use the existing `tool_call_ev` / `tool_result_ev` / `view_from_events` helpers but need a `tool_result_ev` variant carrying custom content + a `StructuredOutput` call. Add a local helper + tests:

```rust
/// tool_result with arbitrary error content (the default helper hardcodes
/// "error output"); lets us drive classify_failure via the excerpt.
fn tool_result_ev_content(i: usize, tool_use_id: &str, content: &str) -> ObservedEvent {
    ObservedEvent {
        tool_use_id: Some(tool_use_id.into()),
        payload: json!({
            "content_ordinal": 0,
            "tool_result": {
                "type": "tool_result",
                "tool_use_id": tool_use_id,
                "is_error": true,
                "content": content
            }
        }),
        ..base_event(i, Actor::Tool, EventKind::ToolResult)
    }
}

/// A StructuredOutput failure is emitted but tagged internal_retry / info,
/// NOT high — so it never enters a severity=high headline (spec §6.3).
#[test]
fn structured_output_failure_is_info_internal_retry() {
    let events = vec![
        tool_call_ev(0, "tid_0", "StructuredOutput"),
        tool_result_ev_content(1, "tid_0", "schema validation failed"),
    ];
    let cands = ToolFailure.extract(&view_from_events(&events));
    assert_eq!(cands.len(), 1);
    assert_eq!(cands[0].severity, "info", "internal retry must not be high");
    assert_eq!(cands[0].subkind, Some("internal_retry"));
    assert_eq!(
        cands[0].evidence_projection["failure_class"].as_str(),
        Some("internal_retry")
    );
}

/// grep no-match is benign → info / benign_nonzero_exit.
#[test]
fn grep_no_match_is_info_benign() {
    let events = vec![
        tool_call_ev(0, "tid_0", "Bash"),
        tool_result_ev_content(1, "tid_0", "grep: no matches found"),
    ];
    let cands = ToolFailure.extract(&view_from_events(&events));
    assert_eq!(cands.len(), 1);
    assert_eq!(cands[0].severity, "info");
    assert_eq!(cands[0].subkind, Some("benign_nonzero_exit"));
}

/// A genuine Bash failure stays high / user_visible.
#[test]
fn real_bash_failure_stays_high_user_visible() {
    let events = vec![
        tool_call_ev(0, "tid_0", "Bash"),
        tool_result_ev_content(1, "tid_0", "error[E0599]: no method named foo"),
    ];
    let cands = ToolFailure.extract(&view_from_events(&events));
    assert_eq!(cands.len(), 1);
    assert_eq!(cands[0].severity, "high");
    assert_eq!(cands[0].subkind, Some("user_visible"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test extractor_tool_failure 2>&1 | tail -20`
Expected: FAIL to **compile** — `FindingCandidate` has no `subkind` field, and the three asserts on `severity`/`subkind` would be wrong (still `"high"` / absent).

- [ ] **Step 3: Add `subkind` to `FindingCandidate`** (`src/insight/types.rs`)

In `FindingCandidate`, add a field after `category`:

```rust
    /// Optional finding sub-type, persisted to `finding.subkind`. For
    /// `tool_failure` this is the `FailureClass` string; `None` otherwise.
    pub subkind: Option<&'static str>,
```

This is a breaking struct change — every existing `FindingCandidate { ... }` literal must set `subkind`. They live in the extractors. After this edit, `cargo build` will list each call site; set `subkind: None` on the four other extractors' candidate literals (`missing_verification.rs`, `risky_action.rs` ×2 branches, `context_bloat.rs`, `final_state_mismatch.rs`) and `noop_test.rs` if present. Use `grep -rn "FindingCandidate {" src/` to enumerate them; add `subkind: None,` to each. (Do NOT touch their behaviour — purely additive default.)

- [ ] **Step 4: Wire the class through `build_candidate`** (`src/insight/extractors/tool_failure.rs`)

The existing `build_candidate` computes `error_excerpt` and `tool_name`. Update its body so the class is computed once and used for `severity`, `subkind`, and the projection. Replace the `let summary = ...; let projection = json!({...}); FindingCandidate { ... }` tail of `build_candidate` with:

```rust
    let class = classify_failure(tool_name, &error_excerpt);

    let summary = format!(
        "Tool {tool_name} failed with is_error=true (tool_use_id={tool_use_id}, class={}).",
        class.as_str()
    );

    let projection = json!({
        "category": "tool_failure",
        "session_id": session_id,
        "failure_class": class.as_str(),
        "tool_use_id": tool_use_id,
        "tool_name": tool_name,
        "error_excerpt_redacted": error_excerpt,
        "tool_result_event_id": result_event_id,
        "paired_call_event_id": call_event_id,
    });

    FindingCandidate {
        category: "tool_failure",
        subkind: Some(class.as_str()),
        confidence_l1: 1.0,
        severity: class.severity(),
        summary,
        evidence_refs,
        evidence_projection: projection,
    }
```

Note: `classify_failure` returns a `FailureClass` whose `as_str()` / `severity()` return `&'static str`, so they fit `subkind: Option<&'static str>` and `severity: &'static str` with no allocation. The no-`tool_use_id` early-fire path in `extract` also calls `build_candidate`, so it gets classified too (its `tool_name` defaults to `"unknown"` → `user_visible`, which is the safe surface-it default).

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --test extractor_tool_failure 2>&1 | tail -25`
Expected: all extractor tests PASS (existing `fires_on_is_error_true_with_no_retry` etc. still pass — `tool_result_ev` content `"error output"` → `user_visible` → `severity "high"`, unchanged).
Then: `cargo build 2>&1 | tail -20` — confirm the other extractors compile after the `subkind: None` additions.

- [ ] **Step 6: Commit**

```bash
git add src/insight/types.rs src/insight/extractors/tool_failure.rs src/insight/extractors/missing_verification.rs src/insight/extractors/risky_action.rs src/insight/extractors/context_bloat.rs src/insight/extractors/final_state_mismatch.rs tests/extractor_tool_failure.rs
git commit -m "feat(tool_failure): classify candidates — info severity + subkind + failure_class projection"
```

(Adjust the `git add` list to exactly the files `grep -rn \"FindingCandidate {\" src/` reported in Step 3.)

---

## Task 4: Persist `subkind` through the pipeline + repo

**Files:**
- Modify: `src/db/repo_finding.rs` (`FindingRow.subkind`, insert, `map_row`, `ListFilter.subkind`, `count_class`)
- Modify: `src/insight/pipeline.rs` (set `subkind` on every `FindingRow` built)
- Modify: `tests/migration_finding_schema.rs` (assert column exists)

- [ ] **Step 1: Write the failing migration-schema test**

`tests/migration_finding_schema.rs` already has `migration_creates_finding_table` which collects `cols: Vec<String>` from `SELECT name FROM pragma_table_info('finding')` after `sqlx::migrate!("./migrations").run(&pool)`. Append a parallel test using that exact pattern (do NOT use `witmcc::db::migrate` here — match the file's `sqlx::migrate!` macro form):

```rust
#[tokio::test]
async fn finding_table_has_subkind_column() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let cols: Vec<String> = sqlx::query_scalar("SELECT name FROM pragma_table_info('finding')")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert!(
        cols.iter().any(|c| c == "subkind"),
        "finding table must have a subkind column; got {cols:?}"
    );
}
```

- [ ] **Step 2: Run test to verify it fails or passes**

Run: `cargo test --test migration_finding_schema 2>&1 | tail -15`
Expected: PASS already (migration 0015 from Task 1 added the column). If it FAILS, the migration didn't apply — re-run `cargo run --bin witmcc -- init-db` is **not** needed for `:memory:` tests since `migrate` runs all migrations; investigate the migration file. This test locks the column for future regressions.

- [ ] **Step 3: Add `subkind` to `FindingRow` + insert + map_row** (`src/db/repo_finding.rs`)

Add the field to `FindingRow` (after `category`):

```rust
    /// Optional finding sub-type (e.g. tool_failure failure class). NULL-able.
    pub subkind: Option<String>,
```

Update `insert` to include the column. Change the SQL + binds:

```rust
pub async fn insert(pool: &SqlitePool, row: &FindingRow) -> Result<()> {
    sqlx::query(
        "INSERT OR REPLACE INTO finding \
         (finding_id, schema_version, session_id, category, subkind, severity, confidence, \
          summary, evidence_refs, evidence_projection, provenance, status) \
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind(&row.finding_id)
    .bind(&row.schema_version)
    .bind(&row.session_id)
    .bind(&row.category)
    .bind(&row.subkind)
    .bind(&row.severity)
    .bind(row.confidence)
    .bind(&row.summary)
    .bind(&row.evidence_refs)
    .bind(&row.evidence_projection)
    .bind(&row.provenance)
    .bind(&row.status)
    .execute(pool)
    .await?;
    Ok(())
}
```

Update `map_row` to read it (note `SELECT *` already returns the new column):

```rust
        subkind: r.get("subkind"),
```

Add the field to `ListFilter`:

```rust
    pub subkind: Option<String>,
```

`ListFilter` derives `Default`, so existing `ListFilter { ..Default::default() }` and `ListFilter { session_id, status, limit, ..Default::default() }` call sites keep compiling. The big match in `list()` does NOT currently branch on subkind; rather than add a fourth dimension to the existing 8-arm match, append a post-filter inside `list()` just before `Ok(rows.into_iter().map(map_row).collect())`:

```rust
    let mut out: Vec<FindingRow> = rows.into_iter().map(map_row).collect();
    if let Some(sk) = &f.subkind {
        out.retain(|r| r.subkind.as_deref() == Some(sk.as_str()));
    }
    Ok(out)
```

(Replace the existing final `Ok(rows.into_iter().map(map_row).collect())` line with the block above. The `LIMIT` was applied in SQL before the in-memory subkind filter; this is acceptable for the count endpoint which uses `count_class` below, and for list drill the limit is 200.)

Add a dedicated count helper at the end of the file (used by the endpoint; counts by class with one SQL aggregate, not limited):

```rust
/// Count active findings for a session+category grouped by subkind.
/// Returns rows of `(subkind_or_null, count)`. Used by the tool-failure
/// summary endpoint so the surface can show user-visible-only counts.
pub async fn count_by_subkind(
    pool: &SqlitePool,
    session_id: &str,
    category: &str,
) -> Result<Vec<(Option<String>, i64)>> {
    let rows = sqlx::query(
        "SELECT subkind AS subkind, COUNT(*) AS n \
         FROM finding \
         WHERE session_id=? AND category=? AND status='active' \
         GROUP BY subkind",
    )
    .bind(session_id)
    .bind(category)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.get::<Option<String>, _>("subkind"), r.get::<i64, _>("n")))
        .collect())
}
```

- [ ] **Step 4: Propagate `subkind` through the pipeline** (`src/insight/pipeline.rs`)

There are three places a `FindingRow` is constructed. Add `subkind` to each:

1. In `build_l1_row` — uses the candidate, so add after `session_id`:

```rust
        subkind: c.subkind.map(|s| s.to_string()),
```

2. In `judge_or_queue` (the L2 promote path) — that `FindingRow { ... }` literal has no candidate-subkind in scope but `c` is available; add:

```rust
        subkind: c.subkind.map(|s| s.to_string()),
```

3. In the pending-queue drain block (`run_extractors_with_runtime` Step 1) — the `FindingRow { finding_id: p.candidate_id.clone(), ... }` there has no candidate; the pending row does not carry subkind. Set:

```rust
        subkind: None,
```

(tool_failure is `PromotionPolicy::Always` → it never enters the judge/pending paths, so paths 2 and 3 only need a compiling default; tool_failure rows always flow through `build_l1_row` / path 1. This is why `None` is correct for the pending path.)

- [ ] **Step 5: Run tests + build**

Run: `cargo build 2>&1 | tail -20` → clean.
Run: `cargo test --test migration_finding_schema --test extractor_tool_failure 2>&1 | tail -20` → PASS.
Run: `cargo test 2>&1 | tail -20` → no regressions. (Watch `tests/api_findings.rs` / `tests/insight_*` — they `INSERT` findings with explicit column lists that omit `subkind`; that is still valid SQL because `subkind` is NULL-able, and `SELECT *` → `map_row` reads it as `None`.)

- [ ] **Step 6: Commit**

```bash
git add src/db/repo_finding.rs src/insight/pipeline.rs tests/migration_finding_schema.rs
git commit -m "feat(finding): persist subkind through pipeline + repo; count_by_subkind + subkind filter"
```

---

## Task 5: Pull API — `subkind` on `FindingDto` + `GET /v1/sessions/:id/tool-failures`

**Files:**
- Modify: `src/api/dto.rs` (`FindingDto.subkind`, `ToolFailureSummaryDto`)
- Modify: `src/api/routes.rs` (`finding_row_to_dto`, `FindingsQuery.subkind`, `session_tool_failures`)
- Modify: `src/api/mod.rs` (route)
- Create: `tests/api_tool_failures.rs`

- [ ] **Step 1: Add the DTOs** (`src/api/dto.rs`)

Add `subkind` to `FindingDto` (after `category`):

```rust
    pub subkind: Option<String>,
```

And add the summary DTO near `FindingsResponse`:

```rust
/// insight-redesign #3 — tool_failure class breakdown for a session.
/// `user_visible` is the only headline-eligible count; the other two are
/// internal/benign noise surfaced for transparency, never lumped into a headline.
#[derive(Serialize)]
pub struct ToolFailureSummaryDto {
    pub session_id: String,
    pub user_visible: i64,
    pub internal_retry: i64,
    pub benign_nonzero_exit: i64,
    /// findings of category tool_failure with NULL subkind (pre-reframe rows
    /// re-ingested before classification, or the no-tool_use_id early path).
    pub unclassified: i64,
    pub total: i64,
    /// The user-visible drill list (full FindingDto rows, severity=high).
    pub user_visible_findings: Vec<FindingDto>,
}

#[derive(Serialize)]
pub struct ToolFailureSummaryResponse {
    pub data: ToolFailureSummaryDto,
}
```

- [ ] **Step 2: Write the failing endpoint test** (`tests/api_tool_failures.rs`)

Mirror `tests/api_findings.rs` exactly for server construction (`AppState::new_for_tests` + `witmcc::api::router(state)` + `TestServer`). Seed three findings directly (one per class). Create `tests/api_tool_failures.rs`:

```rust
//! insight-redesign #3 — GET /v1/sessions/:id/tool-failures returns a class
//! breakdown and a user-visible-only drill list (spec §6.3, Q1/Q2).
use axum_test::TestServer;
use serde_json::Value;
use sqlx::sqlite::SqlitePoolOptions;
use witmcc::api::AppState;
use witmcc::db::migrate;

async fn pool_with_classified_failures() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();

    // (finding_id, subkind, severity)
    for (fid, subkind, sev) in [
        ("find_uv_1", "user_visible", "high"),
        ("find_int_1", "internal_retry", "info"),
        ("find_int_2", "internal_retry", "info"),
        ("find_ben_1", "benign_nonzero_exit", "info"),
    ] {
        sqlx::query(
            "INSERT OR IGNORE INTO finding \
             (finding_id, session_id, category, subkind, severity, confidence, summary, \
              evidence_refs, evidence_projection, provenance, status) \
             VALUES (?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(fid)
        .bind("sess_tf")
        .bind("tool_failure")
        .bind(subkind)
        .bind(sev)
        .bind(1.0_f64)
        .bind("Tool failed.")
        .bind(r#"["ev_001"]"#)
        .bind(format!(r#"{{"category":"tool_failure","failure_class":"{subkind}"}}"#))
        .bind(r#"{"extractor":"tool_failure@v1","layer":"L1","judge":null}"#)
        .bind("active")
        .execute(&pool)
        .await
        .unwrap();
    }
    pool
}

fn build_server(pool: sqlx::SqlitePool) -> TestServer {
    let state = AppState::new_for_tests(pool);
    TestServer::new(witmcc::api::router(state)).unwrap()
}

#[tokio::test]
async fn tool_failures_summary_splits_classes() {
    let server = build_server(pool_with_classified_failures().await);
    let r = server.get("/v1/sessions/sess_tf/tool-failures").await;
    r.assert_status_ok();
    let body: Value = r.json();
    let d = &body["data"];
    assert_eq!(d["user_visible"].as_i64().unwrap(), 1);
    assert_eq!(d["internal_retry"].as_i64().unwrap(), 2);
    assert_eq!(d["benign_nonzero_exit"].as_i64().unwrap(), 1);
    assert_eq!(d["total"].as_i64().unwrap(), 4);
    // The drill list contains only the user_visible finding.
    let list = d["user_visible_findings"].as_array().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["subkind"].as_str().unwrap(), "user_visible");
    assert_eq!(list[0]["severity"].as_str().unwrap(), "high");
}

#[tokio::test]
async fn findings_filter_by_subkind() {
    let server = build_server(pool_with_classified_failures().await);
    let r = server
        .get("/v1/findings?session_id=sess_tf&category=tool_failure&subkind=user_visible&severity=high")
        .await;
    r.assert_status_ok();
    let data = r.json::<Value>()["data"].as_array().unwrap().clone();
    assert_eq!(data.len(), 1);
    assert_eq!(data[0]["subkind"].as_str().unwrap(), "user_visible");
}
```

Run: `cargo test --test api_tool_failures 2>&1 | tail -25`
Expected: FAIL — `FindingDto` has no `subkind` to serialise (compile error in `finding_row_to_dto`) and the route/`subkind` query param are missing.

- [ ] **Step 3: Map subkind + add the query param + handler** (`src/api/routes.rs`)

In `finding_row_to_dto`, add to the `FindingDto { ... }` literal (after `category`):

```rust
        subkind: row.subkind,
```

In `FindingsQuery`, add:

```rust
    pub subkind: Option<String>,
```

In `list_findings`, set it on the filter (after `severity`):

```rust
        subkind: q.subkind,
```

Add the handler near `session_findings`:

```rust
/// `GET /v1/sessions/:id/tool-failures` — tool_failure class breakdown +
/// user-visible drill list (spec §6.3). Internal retries / benign exits are
/// counted but kept out of the drill list so they never headline.
pub async fn session_tool_failures(
    State(pool): State<SqlitePool>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    let counts = match repo_finding::count_by_subkind(&pool, &session_id, "tool_failure").await {
        Ok(c) => c,
        Err(err) => {
            tracing::error!(err = %err, "count_by_subkind failed");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "internal server error"})),
            )
                .into_response();
        }
    };
    let mut user_visible = 0i64;
    let mut internal_retry = 0i64;
    let mut benign = 0i64;
    let mut unclassified = 0i64;
    for (sk, n) in &counts {
        match sk.as_deref() {
            Some("user_visible") => user_visible = *n,
            Some("internal_retry") => internal_retry = *n,
            Some("benign_nonzero_exit") => benign = *n,
            _ => unclassified += *n,
        }
    }
    let total = user_visible + internal_retry + benign + unclassified;

    let drill_filter = repo_finding::ListFilter {
        session_id: Some(session_id.clone()),
        category: Some("tool_failure".into()),
        subkind: Some("user_visible".into()),
        status: Some("active".into()),
        limit: 200,
        ..Default::default()
    };
    let drill = repo_finding::list(&pool, &drill_filter)
        .await
        .unwrap_or_default();
    let user_visible_findings: Vec<FindingDto> =
        drill.into_iter().map(finding_row_to_dto).collect();

    Json(ToolFailureSummaryResponse {
        data: ToolFailureSummaryDto {
            session_id,
            user_visible,
            internal_retry,
            benign_nonzero_exit: benign,
            unclassified,
            total,
            user_visible_findings,
        },
    })
    .into_response()
}
```

`routes.rs` imports DTOs via a glob — `use crate::api::dto::*;` (verified, line 11) — so `ToolFailureSummaryDto` / `ToolFailureSummaryResponse` are in scope automatically once defined in `dto.rs`; no new import line is needed. (`json!`, `StatusCode`, `State`, `Path`, `Json` are likewise already imported and used by the sibling handlers.)

- [ ] **Step 4: Register the route** (`src/api/mod.rs`, immediately after the `/v1/sessions/:id/findings` route at line ~134)

```rust
        .route(
            "/v1/sessions/:id/tool-failures",
            get(routes::session_tool_failures),
        )
```

- [ ] **Step 5: Run tests**

Run: `cargo test --test api_tool_failures 2>&1 | tail -25` → PASS.
Run: `cargo test 2>&1 | tail -15` → no regressions.

- [ ] **Step 6: Commit**

```bash
git add src/api/dto.rs src/api/routes.rs src/api/mod.rs tests/api_tool_failures.rs
git commit -m "feat(api): subkind on FindingDto + GET /v1/sessions/:id/tool-failures summary"
```

---

## Task 6: Frontend type + query wiring

**Files:**
- Modify: `webui/src/api/types.ts`, `webui/src/api/client.ts`, `webui/src/lib/queries.ts`
- Modify: `webui/src/api/__tests__/client.endpoints.test.ts`

- [ ] **Step 1: Add the TS types** (`webui/src/api/types.ts`)

Add `subkind` to `FindingDto` (after `category`):

```typescript
  subkind?: string | null;
```

Add the summary type (near `FindingDto`):

```typescript
export type ToolFailureSummaryDto = {
  session_id: string;
  user_visible: number;
  internal_retry: number;
  benign_nonzero_exit: number;
  unclassified: number;
  total: number;
  user_visible_findings: FindingDto[];
};
```

- [ ] **Step 2: Write the failing client test** (`webui/src/api/__tests__/client.endpoints.test.ts`)

Add `getToolFailureSummary` to the existing import from `../client`, then add this case (it follows the file's `ENVELOPE` / `mockJson` / `fetchSpy` pattern verbatim):

```typescript
describe('getToolFailureSummary', () => {
  it('hits GET /v1/sessions/:id/tool-failures and unwraps `data`', async () => {
    const expected = {
      session_id: 's1', user_visible: 28, internal_retry: 1941,
      benign_nonzero_exit: 12, unclassified: 0, total: 1981, user_visible_findings: [],
    };
    fetchSpy.mockImplementation(mockJson(ENVELOPE(expected)));
    const out = await getToolFailureSummary('s1');
    expect(out).toEqual(expected);
    expect(fetchSpy).toHaveBeenCalledWith(
      expect.stringContaining('/v1/sessions/s1/tool-failures'),
      expect.anything(),
    );
  });
});
```

(If the file's other cases do not pass a second `fetch` arg, drop the `expect.anything()` second arg and match only the URL — copy the exact assertion shape from a sibling `it(...)` in the same file.)

Run: `cd webui && npx vitest run src/api/__tests__/client.endpoints.test.ts 2>&1 | tail -15`
Expected: FAIL — `getToolFailureSummary` is not exported.

- [ ] **Step 3: Add the client fn** (`webui/src/api/client.ts`, near `getFindings`)

```typescript
export const getToolFailureSummary = (id: string): Promise<ToolFailureSummaryDto> =>
  jsonGet<ToolFailureSummaryDto>(`/v1/sessions/${encodeURIComponent(id)}/tool-failures`);
```

Add `ToolFailureSummaryDto` to the type import from `./types`.

- [ ] **Step 4: Add the query hook + key** (`webui/src/lib/queries.ts`)

In `sessionKeys`, add:

```typescript
  toolFailures: (id: string) => ['session', id, 'tool-failures'] as const,
```

Then add the hook (near `useFindingsQuery`):

```typescript
export function useToolFailureSummaryQuery(
  id: string,
  opts?: QOpts<ToolFailureSummaryDto>,
) {
  return useQuery<ToolFailureSummaryDto>({
    queryKey: sessionKeys.toolFailures(id),
    queryFn: () => getToolFailureSummary(id),
    enabled: !!id,
    ...opts,
  });
}
```

Add `getToolFailureSummary` to the imports from `../api/client` and `ToolFailureSummaryDto` from `../api/types`.

- [ ] **Step 5: Run tests + types**

Run: `cd webui && npx vitest run src/api/__tests__/client.endpoints.test.ts 2>&1 | tail -15` → PASS.
Run: `cd webui && npx tsc -b 2>&1 | tail -15` → clean.

- [ ] **Step 6: Commit**

```bash
git add webui/src/api/types.ts webui/src/api/client.ts webui/src/lib/queries.ts webui/src/api/__tests__/client.endpoints.test.ts
git commit -m "feat(webui): tool-failure summary type + getToolFailureSummary + useToolFailureSummaryQuery"
```

---

## Task 7: Re-ingest + manual endpoint smoke + implementation notes

**Files:** `docs/implementation-notes.html` (Modify)

- [ ] **Step 1: Rebuild DB and re-ingest** so existing dev findings gain the `subkind` classification

Run: `cargo run --bin witmcc -- init-db && cargo run --bin witmcc -- ingest --all 2>&1 | tail -5`
Expected: ingest completes; the insight pipeline re-runs and writes `subkind` on tool_failure findings. (Operational note for CLAUDE.md: migration 0015 + re-ingest required — pre-existing findings carry NULL subkind until re-ingested, surfacing as `unclassified` in the summary.)

- [ ] **Step 2: Smoke the endpoint against the real session**

Run: `cargo run --bin witmcc -- serve --bind 127.0.0.1 --port 7878 &` then
`sleep 2 && curl -s http://127.0.0.1:7878/v1/sessions/653ea169-1121-442e-9cc9-776471a10895/tool-failures | python3 -m json.tool | head -40`
Expected: `internal_retry` is the dominant count (≫ `user_visible`), `user_visible` is the small honest number (spec §6.3 anchor ~28), `user_visible_findings` length equals `user_visible`, and every entry has `severity:"high"`, `subkind:"user_visible"`. Confirm `GET /v1/findings?session_id=653ea169-...&severity=high&category=tool_failure` no longer returns the internal-retry flood (those are now `info`). Stop the server afterward.

- [ ] **Step 3: Document in implementation-notes**

Add a new `§` entry to `docs/implementation-notes.html`: the tool_failure reframe (migration 0015 additive `subkind`), the three-class `FailureClass` rule (StructuredOutput → internal_retry; benign-exit markers → benign_nonzero_exit; else user_visible) and the **decision rationale** (option c — keep + tag + demote to `info`, chosen over drop/demote-only to honor the evidence-linked + no-annotation principles and give the API a queryable tag), the `info` severity addition (free TEXT column, no CHECK), `count_by_subkind` + `?subkind=` filter + `/v1/sessions/:id/tool-failures`, and the operational note that `init-db` + re-ingest is required (pre-existing rows → `unclassified`). Note the synthetic-fixture testing fallback (no real fixture has StructuredOutput / is_error=true).

```bash
git add docs/implementation-notes.html
git commit -m "docs(tool_failure): implementation notes for failure-class reframe (slice 3)"
```

---

## Done criteria

- `tool_failure` findings are classified into `user_visible` / `internal_retry` / `benign_nonzero_exit`; only `user_visible` is `severity=high`. Internal `StructuredOutput` retries and benign `grep`/`Read` exits are `severity=info` and never enter a `severity=high` headline.
- The class is persisted on `finding.subkind` (additive migration 0015) and in `evidence_projection.failure_class` (source-preserving). No findings are dropped (evidence-linked + no-annotation principles upheld).
- `GET /v1/sessions/:id/tool-failures` returns per-class counts + a user-visible-only drill list; `GET /v1/findings?...&subkind=user_visible` filters by class.
- Frontend `useToolFailureSummaryQuery` + `getToolFailureSummary` + `FindingDto.subkind` ready for the 도구실패(사용자) card.
- All new tests pass; `cargo test` + (in `webui/`) `npx vitest run` + `npx tsc -b` clean; no regressions.
- Synthetic-fixture testing is flagged (no real fixture has a StructuredOutput / is_error=true failure); the payload *shape* remains real-fixture-anchored via the existing `aac68973-...` reference. Next slice: episode classifier drift fix (§6.4); then the KpiStrip card consumes `useToolFailureSummaryQuery` with a 측정 provenance badge.
