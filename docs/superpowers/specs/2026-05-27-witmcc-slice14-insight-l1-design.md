# Slice-14 Design — Insight Engine v1 (L1 only)

**Date:** 2026-05-27
**Branch (to be cut):** `slice14-insight-l1` off slice-13 merge.
**Goal:** Ship the L1 deterministic insight extractor framework plus the first two finding categories — `missing_verification` and `tool_failure`. Persist `Finding` rows, expose `/v1/findings*` endpoints. **No LLM judge.**

This closes AC-4 ("every finding has evidence_refs and confidence") for the L1-only subset of categories. The judge (L2) lands in slice-15.

---

## 1. Motivation

Per the Insight Engine Architecture spec (`2026-05-27-witmcc-insight-engine-architecture.md`), the engine is split into L1 (deterministic candidate extractor) and L2 (LLM judge). Slice-14 is intentionally the **L1-only** subset because:

- It is the cheapest way to put non-zero findings on the data plane.
- It exercises the full storage path (`Finding` table, Pull API, schema versioning, evidence_refs serialisation) without bringing in the judge's complexity.
- `missing_verification` and `tool_failure` are both flagged by the architecture spec §3 as `Always`-promoted categories (L1 alone is sufficient), making them the natural first targets.

---

## 2. Scope

### In scope

- New table `finding` (migration `0008_finding.sql`).
- `InsightExtractor` trait per architecture spec §4, plus the `SessionInsightView` context object.
- Two implementations:
  - `MissingVerification` — action episode with no following verification episode.
  - `ToolFailure` — `tool_result.is_error == true` with no compensating successful retry.
- Extractor registry (`src/insight/registry.rs`) with the two extractors.
- Pull API endpoints:
  - `GET /v1/findings` (filter by session_id, category, severity, status)
  - `GET /v1/findings/:id`
  - `GET /v1/findings/:id/evidence` (subgraph + raw source refs)
  - `GET /v1/sessions/:id/findings`
- Pipeline wiring: at the end of `rebuild_session`, after `compute()` writes nodes+edges, the extractor pipeline runs and writes finding rows.

### Out of scope

- Judge layer (slice-15).
- `risky_action`, `context_bloat`, `final_state_mismatch` (slice-16).
- `findings_pending_judge` queue (slice-15 owns the table + endpoint).
- UI surface (UX redesign epic).

---

## 3. `Finding` schema

```sql
-- migrations/20260530120000_0008_finding.sql
CREATE TABLE IF NOT EXISTS finding (
    finding_id          TEXT PRIMARY KEY,
    schema_version      TEXT NOT NULL DEFAULT 'finding.v1',
    session_id          TEXT NOT NULL,
    category            TEXT NOT NULL,
    severity            TEXT NOT NULL,
    confidence          REAL NOT NULL,
    summary             TEXT NOT NULL,
    evidence_refs       TEXT NOT NULL,        -- JSON array of event_id strings
    evidence_projection TEXT NOT NULL,        -- JSON object — what the judge saw (L1: the L1-side projection)
    provenance          TEXT NOT NULL,        -- JSON object: { extractor, layer, judge, judge_template_version, rule_pack }
    status              TEXT NOT NULL DEFAULT 'active',  -- "active" | "pending_judge" | "discarded"
    created_at          TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_finding_session ON finding(session_id);
CREATE INDEX IF NOT EXISTS idx_finding_category ON finding(category);
CREATE INDEX IF NOT EXISTS idx_finding_severity_session ON finding(severity, session_id);
```

`finding_id` derivation: `"find_" + sha256(category || "\0" || session_id || "\0" || sorted_evidence_refs_join)`. This makes the ID idempotent — re-running the extractor on the same evidence produces the same ID, so `INSERT OR REPLACE` is safe.

---

## 4. Category 1 — `missing_verification`

### Rule (deterministic)

For each `action` episode `A` (slice-12), let `W = [intake_i, next_intake_or_end)` be the intake window containing `A`. The rule fires iff:

1. `A` has at least one `diff_hunk` produced inside its event range.
2. There is **no** `verification` episode in `W` strictly after `A.end_event_id`.

### Confidence

Fixed `0.9`. Promotion policy: `Always`.

### Evidence

```json
{
  "category": "missing_verification",
  "session_id": "...",
  "action_episode_id": "ep_...",
  "introduced_diff_hunks": ["dh_...", "dh_..."],
  "intake_window": { "start_event_id": "ev_...", "end_event_id": "ev_..." }
}
```

`evidence_refs` contains: the action episode's `start_event_id` + `end_event_id`, every diff_hunk's `introduced_by_event_id`, and the intake window's `start_event_id`.

### Edge cases

- No `action` episode in the session ⇒ rule emits zero candidates. (Test fixture: a read-only session.)
- Action episode is also the last episode of the session ⇒ rule fires (no verification window exists). Confidence still 0.9.
- The session has an `action` episode immediately followed by a `verification` episode with `status == "failed"` ⇒ rule does **not** fire (a verification existed). The `final_state_mismatch` finding (slice-16) handles failed-verification cases.

---

## 5. Category 2 — `tool_failure`

### Rule

For each `tool_result` event whose payload has `is_error == true`:

1. Walk forward in the same session.
2. If within `M` events (`M = 5` for v1) there is another `tool_result` for the same `tool_use_id` with `is_error == false`, the rule does **not** fire (a retry succeeded).
3. Otherwise the rule fires.

### Confidence

Fixed `1.0` (we are quoting the error flag verbatim).

### Severity

`high`.

### Evidence

```json
{
  "category": "tool_failure",
  "session_id": "...",
  "tool_use_id": "toolu_...",
  "tool_name": "Bash",
  "command_or_input_excerpt_redacted": "<first 256 bytes of input, redacted>",
  "error_excerpt_redacted": "<first 512 bytes of error, redacted>",
  "tool_result_event_id": "ev_..."
}
```

`evidence_refs`: just `[tool_result_event_id, paired_tool_call_event_id]`.

### Edge cases

- Multiple `tool_result` events for the same `tool_use_id` (matched call) — slice-1's merge already collapses these onto one node; the rule reads the **final** is_error.
- `is_error` flag absent ⇒ treat as `false`. No fire.
- Bash tool that intentionally errors (e.g., `grep -q` returning non-zero) — out of MVP. The current implementation fires for these; the noise is acknowledged and a future v2 may add a "this is exit-code-shaped, not a failure" judge gate.

---

## 6. Extractor pipeline

```rust
// src/insight/pipeline.rs
pub async fn run_extractors(
    pool: &SqlitePool,
    session_id: &str,
) -> Result<Vec<FindingRow>> {
    let view = SessionInsightView::load(pool, session_id).await?;
    let extractors = all_extractors();  // from registry
    let mut findings = Vec::new();
    for ext in extractors {
        let cands = ext.extract(&view);
        for c in cands {
            if c.confidence_l1 < 0.5 { continue; }
            findings.push(FindingRow::from_candidate(c, ext.category()));
        }
    }
    // Idempotent: dedupe by finding_id then INSERT OR REPLACE
    write_findings_in_tx(pool, &findings).await?;
    Ok(findings)
}
```

`SessionInsightView::load` reads `observed_event`, `diff_hunk`, `verification_run`, `episode`, `graph_node`, `graph_edge` into in-memory Vecs once. The trait method `extract()` then works against the loaded view — pure-CPU, deterministic, idempotent.

### Wiring into `rebuild_session`

```rust
pub async fn rebuild_session(pool: &SqlitePool, session_id: &str) -> Result<(usize, usize, usize)> {
    let evs = repo_observed::list_session(pool, session_id, 100_000).await?;
    let hunks = repo_diff_hunk::list_session(pool, session_id).await?;
    let runs = repo_verification_run::list_session(pool, session_id).await?;
    let (nodes, edges) = compute(session_id, &evs, &hunks, &runs);
    // graph commit
    let mut tx = pool.begin().await?;
    repo_graph::delete_session_in_tx(&mut tx, session_id).await?;
    repo_graph::insert_nodes_edges_in_tx(&mut tx, &nodes, &edges).await?;
    tx.commit().await?;
    // episode + finding pass (separate transactions — order matters; findings read episodes)
    let _eps = run_episode_classifier_and_persist(pool, session_id).await?;
    let findings = run_extractors(pool, session_id).await?;
    Ok((nodes.len(), edges.len(), findings.len()))
}
```

The new return tuple shape is reflected in `tests/graph_build.rs` updates.

---

## 7. Pull API surface

### `GET /v1/findings`

Query params:
- `session_id` (optional)
- `category` (optional)
- `severity` (optional — `high | medium | low`)
- `status` (optional — `active | pending_judge | discarded`; defaults to `active`)
- `cursor` (optional — opaque pagination)
- `limit` (optional — default 50, max 200)

Response:

```json
{
  "data": [
    {
      "finding_id": "find_...",
      "schema_version": "finding.v1",
      "session_id": "sess_...",
      "category": "missing_verification",
      "severity": "medium",
      "confidence": 0.9,
      "summary": "Action episode ep_… had no following verification.",
      "evidence_refs": ["ev_...", "ev_..."],
      "evidence_projection": { ... },
      "provenance": {
        "extractor": "missing_verification@v1",
        "layer": "L1",
        "judge": null
      },
      "status": "active",
      "created_at": "..."
    }
  ],
  "meta": { ... }
}
```

### `GET /v1/findings/:id`

Same shape, single object.

### `GET /v1/findings/:id/evidence`

```json
{
  "data": {
    "finding": { /* same as above */ },
    "subgraph": {
      "nodes": [ { /* graph nodes whose ids appear in evidence_refs or whose source_event_ids include any evidence_refs entry */ } ],
      "edges": [ { /* edges connecting those nodes */ } ]
    },
    "raw_source_refs": [
      { "event_id": "ev_...", "source_type": "claude_transcript", "source_uri": "file://...", "redaction_state": "..." }
    ]
  }
}
```

### `GET /v1/sessions/:id/findings`

Alias for `GET /v1/findings?session_id=:id`.

---

## 8. Severity mapping

| Category | Severity |
|---|---|
| `missing_verification` | `medium` |
| `tool_failure` | `high` |
| (future) `risky_action` | `high` |
| (future) `context_bloat` | `low` |
| (future) `final_state_mismatch` | `medium` |

Severity is a constant per category in slice-14. No per-finding severity inference.

---

## 9. Failure modes

| Failure | Behaviour |
|---|---|
| Extractor panics | `catch_unwind`; log + zero findings from that extractor for that session. |
| Evidence ref points to a deleted event (post-retention sweep) | Finding excluded from `/v1/findings` results but kept in DB until retention sweeps the finding itself. |
| Session has no episodes | Both extractors emit zero findings. |
| Two pipeline runs produce same finding_id | `INSERT OR REPLACE` keeps the later row (identical content). |
| Pipeline takes longer than 5 s on `aac68973` | Smoke records the time; if exceeded the slice plan flags it for profiling in the PR description. |

---

## 10. Deviations index (slice-14)

| ID | Description |
|---|---|
| DEV-S14-01 | Only two categories ship. The other three named in the architecture spec are slice-16 work. |
| DEV-S14-02 | No judge layer in this slice. `provenance.judge` is always `null` for L1 findings (locked by test). |
| DEV-S14-03 | `evidence_projection` is stored on every finding even though L1 categories do not need it for L2 (there is no L2). This is intentional: the column shape is stable from slice-14 onward so slice-15 does not need a migration. |
| DEV-S14-04 | `tool_failure` fires on Bash exit-code-shaped failures (`grep -q` returning 1 on no-match). This noise is acknowledged; future v2 may judge-gate it. |
| DEV-S14-05 | The `rebuild_session` signature widens from `(nodes, edges)` to `(nodes, edges, findings)`. All existing callers are updated in this slice. |
| DEV-S14-06 | The extractor pipeline runs **after** the graph rebuild transaction commits, not inside it. This is to keep the graph commit small and atomic (slice-9 invariant); findings live in their own transaction. |
| DEV-S14-07 | `finding.status` defaults to `"active"` for L1 findings. The `"pending_judge"` value is reserved for slice-15. The enum is locked from slice-14 to avoid a migration in slice-15. |

---

## 11. Commit plan

See `2026-05-27-witmcc-slice14-insight-l1.md`. Six commits:

1. `test(slice-14): red-locking tests for insight pipeline + finding schema`
2. `feat(db): 0008_finding migration + repo_finding`
3. `feat(insight): InsightExtractor trait + SessionInsightView + registry`
4. `feat(insight): MissingVerification + ToolFailure extractors`
5. `feat(api): /v1/findings*, /v1/sessions/:id/findings, /v1/findings/:id/evidence`
6. `feat(graph): wire insight pipeline into rebuild_session`
