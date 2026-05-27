# Slice-13 Design — Causal-edge Inference

**Date:** 2026-05-27
**Branch (to be cut):** `slice13-causal-edge-inference` off slice-12 merge.
**Goal:** Add **inferred** edges to the graph beyond the deterministic edges built in slice-1..11. Each inferred edge carries an `inference_rule_id` (version-suffixed) and a numerical `confidence`. The first version (`_v1`) ships three rules, each pinned by a frozen count over real transcripts.

This slice closes the M3 milestone (basic causal inference).

---

## 1. Motivation

The current graph builder is dominated by **deterministic** edges — `tool_call_to_result`, `message_reply`, `triggered_verification`. These are correct but they do not capture the causal narrative a reader actually wants: *"this Bash failure caused the next Read"*, *"the user asked for X and Y started immediately after"*, *"the giant grep output is why the next assistant turn went off-topic"*.

Adding those edges as **inferred** (with explicit `confidence` and a versioned `rule_id`) lets the UI surface them as visually distinct lines and lets the Why Panel cite the rule that fired. The data-model spec already calls for `inferred edge must show confidence` (`docs/01_product_design_spec.html` §5).

---

## 2. Scope

### In scope

- New columns on `graph_edge`: `inference_rule_id TEXT NULL`, `confidence REAL NULL` (migration `0007_graph_edge_inference.sql`).
- Three rules in `src/insight/edge_inference/`:
  - `rules/caused_repair_v1.rs`
  - `rules/triggered_by_user_message_v1.rs`
  - `rules/large_output_to_next_action_v1.rs`
- A rule registry mirroring slice-12's pattern (`pub const RULE_IDS: &[&str]`).
- `compute()` extended to pipe through `&[ObservedEvent]`, `&[GraphNode]`, `&[GraphEdge]` to each rule. Each rule returns `Vec<GraphEdge>` (with `inference_rule_id` + `confidence` populated) that are appended after deterministic edges.
- Frozen golden file `tests/fixtures/inferred_edge_counts.json` mapping `(session_id, rule_id) → count`. Updates require commit-body deviation note.

### Out of scope

- Cross-session inferred edges.
- Learned thresholds (all thresholds are constants per rule version).
- Bidirectional inferred edges (an inferred edge has a directed `from → to`).
- Inferred edges that span Out-of-OTel sources (we do not infer across sessions even if `trace_id` matches).
- UI visualisation — UX redesign epic owns that.

---

## 3. Inference rules (v1)

Each rule is a pure function on the **complete** session view:

```rust
pub trait EdgeInferenceRule: Send + Sync {
    fn rule_id(&self) -> &'static str;
    fn infer(&self, view: &SessionGraphView) -> Vec<GraphEdge>;
}

pub struct SessionGraphView<'a> {
    pub session_id: &'a str,
    pub events: &'a [ObservedEvent],
    pub nodes: &'a [GraphNode],
    pub deterministic_edges: &'a [GraphEdge],
}
```

### 3.1 `caused_repair@v1`

**From:** a `tool_call` whose paired `tool_result.is_error == true`.
**To:** the **next** `tool_call` in the same session within `N` seconds (`N = 60` for v1) whose input text shares ≥ `K` tokens with the failing `tool_result`'s stderr/error text (`K = 2` tokens, after stop-word removal).
**Confidence:** `0.7 × overlap_score + 0.3 × time_decay`. `overlap_score ∈ [0, 1]` is Jaccard over the set of matched tokens. `time_decay ∈ [0, 1]` decays linearly from 1.0 at `Δt = 0` to 0.0 at `Δt = N`.

**Edge attributes:**

```json
{
  "rule_id": "caused_repair@v1",
  "confidence": 0.82,
  "matched_terms": ["NoMethodError", "users.py"],
  "delta_seconds": 11
}
```

### 3.2 `triggered_by_user_message@v1`

**From:** a `user_message` node.
**To:** the next `tool_call` in the same session that has **no preceding `assistant_message` in the same turn**. This catches slash-command-shaped invocations (user types `/run-tests`, Claude jumps directly to Bash) and other cases where the assistant text was a single tool-use.
**Confidence:** fixed `0.85` when the rule fires.

**Edge attributes:**

```json
{
  "rule_id": "triggered_by_user_message@v1",
  "confidence": 0.85
}
```

### 3.3 `large_output_to_next_action@v1`

**From:** a `tool_result` whose payload byte size exceeds `T` (`T = 50 * 1024` bytes for v1).
**To:** the next `assistant_message` in the same session.
**Confidence:** `0.6 + 0.4 × normalise(payload_size / max_observed_session)`. Clamped to `[0.6, 1.0]`.

**Edge attributes:**

```json
{
  "rule_id": "large_output_to_next_action@v1",
  "confidence": 0.78,
  "tool_result_size_bytes": 87340
}
```

This rule is **only an inferred edge** — it does not promote the candidate to a `context_bloat` finding. That promotion is slice-16's `context_bloat` extractor.

---

## 4. Schema change

```sql
-- migrations/20260529120000_0007_graph_edge_inference.sql
ALTER TABLE graph_edge ADD COLUMN inference_rule_id TEXT;
ALTER TABLE graph_edge ADD COLUMN confidence REAL;

CREATE INDEX IF NOT EXISTS idx_graph_edge_rule
    ON graph_edge(inference_rule_id);
```

ALTER TABLE ADD COLUMN with no default is safe in SQLite. Existing deterministic edges retain `NULL` in both columns. The test `tests/migration_inference_columns.rs` confirms.

`GraphEdge` struct in `src/model/graph.rs` gains:

```rust
#[serde(skip_serializing_if = "Option::is_none")]
pub inference_rule_id: Option<String>,
#[serde(skip_serializing_if = "Option::is_none")]
pub confidence: Option<f32>,
```

For deterministic edges built before slice-13, the values stay `None`; the JSON omits the keys.

---

## 5. Frozen counts golden

`tests/fixtures/inferred_edge_counts.json`:

```json
{
  "schema_version": "inferred_edge_counts.v1",
  "by_session_and_rule": {
    "aac68973": {
      "caused_repair@v1": <N>,
      "triggered_by_user_message@v1": <M>,
      "large_output_to_next_action@v1": <K>
    }
  }
}
```

Bootstrap in commit 4 same as slice-12's golden bootstrap. Test `tests/inferred_edge_counts.rs` asserts exact match. Updating the file requires a deviation note.

---

## 6. Why versioned rule IDs

Every rule's threshold (window size, byte threshold, Jaccard token count, time decay) lives in **constants inside the rule file**. Changing a constant requires bumping the version (`@v1` → `@v2`) and:

1. Creating a new rule file (do **not** edit `_v1.rs`).
2. Registering both in `RULE_IDS`. The old version may eventually be retired in a separate slice that includes a migration of stored edges.
3. Updating the golden counts.

Effect: a future reviewer can `git blame` `caused_repair_v1.rs` and trust the rule is unchanged since merge. If they see `caused_repair_v2.rs` they know the constants moved.

---

## 7. Failure modes

| Failure | Behaviour |
|---|---|
| Rule panics | `catch_unwind` per rule, log + emit zero edges from that rule for that session; other rules continue. |
| Rule produces an edge whose `from`/`to` node IDs are not in the graph | Edge dropped, audit row written (`graph_edge_orphan_inferred`). |
| Two rules produce identical `(from, to, rule_id)` edges | Dedupe at insert time; only the higher-confidence wins. |

---

## 8. Deviations index (slice-13)

| ID | Description |
|---|---|
| DEV-S13-01 | Inferred edges are appended **after** deterministic edges in `compute()`. Ordering matters for tests that compare edge lists; the test fixtures account for this. |
| DEV-S13-02 | Inferred edge thresholds (`N=60s`, `K=2 tokens`, `T=50KB`) are constants per rule version. No runtime configuration. |
| DEV-S13-03 | `caused_repair@v1` uses a **lexical** overlap rule. No model call. The L2 judge architecture is not used here — the goal is fast deterministic candidate edges. Semantic matching is a possible v2. |
| DEV-S13-04 | The `triggered_by_user_message@v1` rule does not fire on every `user_message → tool_call` pair, only those without a preceding `assistant_message` in the same turn. The common path (`user → assistant text → tool_call`) is already covered by `message_reply` deterministic edges. |
| DEV-S13-05 | `large_output_to_next_action@v1` does **not** create a finding. It only produces an edge. The `context_bloat` finding category (slice-16) reads these edges as candidate evidence. |
| DEV-S13-06 | `confidence` is a `REAL` (f32 in Rust), not a fixed-point value. Tests assert with epsilon (`±0.01`). |

---

## 9. Commit plan summary

See `2026-05-27-witmcc-slice13-causal-edge-inference.md`. Five commits:

1. `test(slice-13): red-locking tests for inferred edges + rule registry + schema`
2. `feat(db): 0007_graph_edge_inference migration`
3. `feat(insight): three inferred-edge rules (caused_repair, triggered_by_user_message, large_output_to_next_action)`
4. `feat(insight): inferred_edge_counts golden bootstrap`
5. `feat(graph): wire inferred edges into compute() + rebuild_session`
