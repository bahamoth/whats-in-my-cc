# Insight Engine Architecture

**Date:** 2026-05-27
**Status:** Design spec — locks the L1/L2 split before slice-14 implementation starts.
**Companion roadmap:** `2026-05-27-witmcc-remaining-milestones-roadmap.md`
**Scope of decisions in this file:** every architectural decision that, if revisited mid-implementation, would force code-wide rewrites. Per-category rule details belong to the category's own slice (14/16) design doc.

---

## 0. Constraint from user direction (2026-05-27)

Two failure modes were explicitly forbidden by the user:

1. **LLM-per-event judgment.** Calling an LLM on every observed event (or every finding candidate emitted in real time) is rejected as cost-unsafe.
2. **Pure deterministic rules.** Sufficiently rigid rules (e.g., "tool_result with stderr ⇒ tool_failure finding") produce too many false positives or miss too many context-dependent cases.

The architecture below picks a two-layer composition that satisfies both: a **L1 deterministic extractor** runs on every rebuild (cheap, idempotent, evidence-complete), and a **L2 LLM judge** runs **only on a subset of L1 candidates** for a subset of categories, with caching and a per-session budget cap. The split is per-category, not global.

---

## 1. Why a two-layer split (not one or the other)

| Approach | Cost profile | False-positive control | Why rejected as the only layer |
|---|---|---|---|
| Pure deterministic rules | $0 LLM | Brittle thresholds; e.g., "command output >10 KB is bloat" hits any normal `grep` output | One-layer-only loses categories where the meaning depends on whether the bloat was *used*. |
| Per-candidate LLM call (no rules) | Linear in events | Highest quality | Cost: ~10 ¢ per 1 000 events at Sonnet-class pricing × N sessions × N rebuilds. User-forbidden. |
| LLM-as-summariser only (no extractor) | Linear in sessions, not events | Loses evidence anchoring (model paraphrases) | Violates Evidence-linked principle in `docs/03_data_model_spec.html` §1. |
| **L1 deterministic + L2 LLM judge on subset** | $0 baseline; LLM only on L1-flagged candidates above a confidence band | Best of both — cheap candidate generation, judge resolves ambiguity | **Chosen.** |

The chosen architecture also degrades gracefully: with the L2 judge disabled (default off in slice-15), the engine still emits findings — just only the categories that L1 alone can fill (`missing_verification`, `tool_failure`).

---

## 2. Layer responsibilities

### L1 — Deterministic candidate extractor

- Runs on every `rebuild_session(session_id)` invocation, immediately after graph rebuild.
- Pure CPU work, no I/O beyond reading `observed_event` / `verification_run` / `episode` / `diff_hunk` / `graph_node` / `graph_edge` rows already in the DB.
- Output: a list of `FindingCandidate { category, evidence_refs, confidence_l1, raw_signals }` — *not* `Finding` rows. Candidates with `confidence_l1 >= 1.0 - epsilon` are auto-promoted to `Finding` directly. Lower-confidence candidates are either dropped (below floor) or queued for L2.
- Idempotency: a candidate's identity is `(category, ordered evidence_refs hash)`. Re-running the extractor produces the same identity, allowing dedupe and cache lookup.

### L2 — LLM judge (optional, per-category)

- Consumes the queue of L1 candidates whose category is configured to use the judge.
- Each candidate becomes a single chat call with a structured prompt:
  - The prompt includes the candidate's category, its evidence (a compact projection — see §6), and a structured-output schema.
  - The model is **always Sonnet-class** for cost; categories that genuinely benefit from Opus are out-of-scope for MVP.
- Output: `JudgeVerdict { promote: bool, confidence_l2: f32, mismatch_summary?: String, reason: String }`. `promote == true` ⇒ insert as `Finding`. `promote == false` ⇒ candidate discarded (logged in audit).
- Budget gate: a session-scoped counter caps total judge calls per `rebuild_session` invocation. Defaults to **0 (off)** until the user opts in.

### What the judge is **not**

- Not a summariser. It returns a verdict on a candidate the extractor produced; it does not generate findings from raw events.
- Not a fact extractor. `evidence_refs` are fixed by L1; the judge cannot add evidence the extractor did not see.
- Not a retroactive judge. It runs at rebuild time, not at user-view time, so view-time UX does not pay LLM latency.

---

## 3. Per-category routing

This table is canonical. Changes require a design-doc deviation in this file plus a slice-doc note.

| Category | L1 rule | L2 judge role | Default state |
|---|---|---|---|
| `missing_verification` | Action episode A in window [intake_i, intake_{i+1}) has no following verification episode in the same window. | None — L1 sufficient. | Always on. |
| `tool_failure` | `tool_result.is_error == true` and no compensating successful retry within window. | None — `is_error` is authoritative. | Always on. |
| `risky_action` | (a) Destructive Bash command (`rm -rf`, `git push --force`, etc.) on allowlist, OR (b) `diff_hunk.user_modified == true`. | Judge decides: "was the action user-directed and intentional, or unsupervised drift?" | Off by default (slice-16 ships rule; judge gating is opt-in). |
| `context_bloat` | Single `tool_result.payload_size_bytes > threshold` followed within N events by an `assistant_message` whose token estimate is high. | Judge decides: was the bloat reused (worth keeping) or wasted? | Off by default. |
| `final_state_mismatch` | `user_message` contains explicit goal hint (frozen lexicon `"fix"`, `"add"`, `"remove"`, `"make … work"`); final `assistant_message` + closing `verification_run.status` do not corroborate completion. | Judge produces structured `mismatch_summary` and verdict on whether mismatch is real. | Off by default. |

**Per-category confidence policy:**
- `missing_verification`: `confidence_l1 = 0.9`. No judge override; finding stored at 0.9.
- `tool_failure`: `confidence_l1 = 1.0` (we are quoting the error flag).
- `risky_action`: `confidence_l1 = 0.7`. Judge promote → `confidence_l2` in `[0.7, 1.0]`; judge discard drops the candidate.
- `context_bloat`: `confidence_l1 = 0.5`. Judge promote → `confidence_l2` reported by judge; the bottom-line confidence stored is `confidence_l2`, never `confidence_l1`. Judge discard drops.
- `final_state_mismatch`: `confidence_l1 = 0.6`. Judge promote → `confidence_l2` from judge.

**Floor:** any candidate with final stored confidence `< 0.5` is dropped, regardless of which layer set it.

---

## 4. `InsightExtractor` trait

Each L1 category is implemented as a struct that implements:

```rust
pub trait InsightExtractor: Send + Sync {
    /// Stable string id — appears in `Finding.category`.
    fn category(&self) -> &'static str;

    /// L1 confidence floor for this category — judge cannot lower past this.
    fn floor(&self) -> f32;

    /// Whether this category requires L2 judge before promotion.
    /// `Always`  — L1 always promotes.
    /// `Never`   — L1 never alone promotes (will be dropped if judge off).
    /// `IfAbove(threshold)` — L1 promotes alone above threshold, otherwise judge.
    fn promotion_policy(&self) -> PromotionPolicy;

    /// Pure CPU work — extract candidates from already-loaded session view.
    fn extract(&self, view: &SessionInsightView) -> Vec<FindingCandidate>;
}

pub enum PromotionPolicy {
    Always,
    Never,
    IfAbove(f32),
}
```

`SessionInsightView` is the *single* read-only context object handed to every extractor in a rebuild. It is constructed once per `rebuild_session` call and contains:

```rust
pub struct SessionInsightView<'a> {
    pub session_id: &'a str,
    pub events: &'a [ObservedEvent],
    pub diff_hunks: &'a [DiffHunkRow],
    pub verification_runs: &'a [VerificationRunRow],
    pub episodes: &'a [EpisodeRow],
    pub nodes: &'a [GraphNode],
    pub edges: &'a [GraphEdge],
}
```

This shape forces extractors to be pure functions of locked input — no DB calls inside `extract()`, no side effects, no network. Testing is trivial: build a `SessionInsightView` with fixture data, call `extract()`, assert on the returned candidates.

### Trait registry

```rust
// src/insight/registry.rs
pub fn all_extractors() -> Vec<Box<dyn InsightExtractor>> {
    vec![
        Box::new(MissingVerification),  // slice-14
        Box::new(ToolFailure),          // slice-14
        Box::new(RiskyAction),          // slice-16
        Box::new(ContextBloat),         // slice-16
        Box::new(FinalStateMismatch),   // slice-16
    ]
}
```

A test in `tests/insight_registry.rs` asserts that this registry has exactly the categories listed in §3's table, in the listed order, by category id. Adding/removing a category without updating the table fails the test.

---

## 5. L2 judge interface

### Trait

```rust
#[async_trait::async_trait]
pub trait JudgeProvider: Send + Sync {
    async fn judge(&self, prompt: JudgePrompt) -> Result<JudgeVerdict>;

    /// Stable, version-suffixed name (e.g., "anthropic-sonnet-4-6").
    /// Surfaced in stored Finding.provenance.
    fn model_id(&self) -> &str;

    /// Used for cache namespacing. Bump when prompt template changes.
    fn prompt_template_version(&self) -> &str;
}
```

### Implementations

- `AnthropicJudge` — real Anthropic SDK call. Uses prompt caching with cache_control on the system prompt + category schema (these are stable across calls). Maintained alongside `claude-api` skill conventions.
- `NoopJudge` — returns `promote: false, reason: "judge_disabled"` for every input. Used when the user has not opted into LLM judgement. The runtime is configured to skip categories that require Judge when this implementation is active.
- `FixtureJudge` — test-only. Reads a JSON file mapping `(category, evidence_hash)` → `JudgeVerdict`. Lets tests assert end-to-end without touching the network.

### Cache

Cache key:

```
sha256(category || "\0" || model_id || "\0" || prompt_template_version || "\0" || evidence_hash)
```

where `evidence_hash` is the SHA-256 of the canonical-JSON serialisation of the candidate's evidence projection (see §6). Identical candidates against the same judge + template never call the API a second time.

Cache storage: `judge_verdict_cache` SQLite table — see slice-15 design for schema. Cache is **session-scoped** in lookup (the same evidence in a different session is a different cache key because session_id is part of the evidence projection), but cross-session in storage so the same configuration across sessions can share results when evidence happens to match.

### Budget

`max_judge_calls_per_session: usize` — set via CLI flag `--judge-budget` or env `WITMCC_JUDGE_BUDGET`. Defaults:

- **0** when no judge configured (i.e., NoopJudge active).
- **20** when a real judge is active. Sized so a single rebuild of `aac68973` (which currently has ~1500 graph nodes) cannot blow the user's wallet without explicit elevation.

When the budget is exhausted mid-rebuild, remaining candidates are **kept in the queue** and stored as `findings_pending_judge` (a side-table). The next `rebuild_session` invocation that has budget continues from the queue. This guarantees that even with an aggressive budget the system makes monotone progress and never silently drops candidates.

### Concurrency

Judge calls within a single rebuild are issued sequentially. Parallel calls are out-of-scope for MVP — the budget cap is small enough that sequential is fast enough, and parallel introduces ordering issues with the cache write path.

---

## 6. Evidence projection (what the judge sees)

The judge never sees the full event payloads. Each category defines a `EvidenceProjection` — a `serde_json::Value` containing only the fields relevant to that category. The projection is what gets:

1. Serialised into the LLM prompt (token-budgeted).
2. Hashed for cache key.
3. Persisted onto the stored `Finding` so the user can audit what the judge saw.

### Projection contract

```rust
pub trait EvidenceProjection {
    /// Stable, lossless projection of the candidate's evidence into a JSON
    /// value safe to send to an LLM. **Must not** include raw secret payloads;
    /// the redaction manifest (slice-18) is applied before projection by the
    /// redaction gate.
    fn project(&self, candidate: &FindingCandidate, view: &SessionInsightView) -> Value;
}
```

### Example — `risky_action` projection

```json
{
  "category": "risky_action",
  "session_id": "sess_…",
  "trigger": {
    "kind": "destructive_bash",
    "command_redacted": "rm -rf <path>",
    "tool_use_id": "toolu_…",
    "tool_result_event_id": "ev_…",
    "introduced_diff_hunks": [
      { "diff_hunk_id": "dh_…", "file_path_redacted": "<repo>/src/x.rs", "lines_removed": 17 }
    ]
  },
  "context": {
    "preceding_user_message_excerpt_redacted": "...",
    "preceding_assistant_message_excerpt_redacted": "...",
    "episode_phase": "action"
  }
}
```

Every string field that could carry secrets goes through the slice-18 redaction gate before projection. The projection itself is what the prompt template references — the template never has direct access to raw events.

---

## 7. False positive control

### Per-category gold sets

Each category has a frozen golden file `tests/fixtures/insight_gold/<category>.json` containing:

```json
{
  "session_fixture": "<path/to/transcript fixture>",
  "expected_findings": [
    { "category": "...", "evidence_refs": [...], "confidence_floor": 0.7 }
  ],
  "expected_judge_calls": 2
}
```

A test in `tests/insight_gold.rs` runs the full extractor pipeline against each fixture (using `FixtureJudge` keyed to a pre-recorded judge response file) and asserts the produced findings match exactly. Updating the golden file requires a commit body line `(insight gold update: <category> — <reason>)`.

### Confidence floor (re-stated)

Any candidate whose final stored confidence is `< 0.5` is dropped before insert. The floor lives in `src/insight/floor.rs` as a constant, and is re-asserted in every category test.

### No-emit invariant tests

For each category, at least one fixture exists where the extractor must emit **zero** candidates. This guards against rule overreach.

Examples:
- `missing_verification` zero-emit fixture: a session where every action episode is followed by a verification episode.
- `tool_failure` zero-emit fixture: a session where every tool_result has `is_error == false`.
- `risky_action` zero-emit fixture: a session with only read-only tool calls.
- `context_bloat` zero-emit fixture: a session whose largest tool_result is below threshold.
- `final_state_mismatch` zero-emit fixture: a session where the closing verification succeeds.

---

## 8. Cost model & opt-in default

### Cost analysis (Sonnet-class pricing assumed)

- Each judge call: ~3 000 input tokens (system + projection + schema) × ~$3/Mtoken = $0.009 + ~400 output tokens × ~$15/Mtoken = $0.006 ≈ **$0.015 per call**.
- Default budget 20 calls/session ⇒ **≤ $0.30 per session**.
- With prompt caching on the stable header (system + schema), cached input ≈ 90 % of input, reducing input cost roughly 5×. Effective per-call cost target: **≤ $0.005**.
- 100 sessions/day at default budget ⇒ **≤ $30/day** before caching, **≤ $10/day** after. This is the worst-case if the user opts in to judge on every session — well within an individual developer's tolerance, but presented up-front so the opt-in is informed.

### Opt-in default policy

- **First run:** `witmcc serve` prints `LLM judge: disabled (NoopJudge). Run with --judge anthropic to enable.` on stderr.
- **Opt-in flag:** `--judge anthropic` activates `AnthropicJudge`. Requires `ANTHROPIC_API_KEY` env var; refuses to start if missing.
- **Budget flag:** `--judge-budget N` (default 20). `0` is equivalent to NoopJudge.
- **No silent fallback:** if the configured judge fails mid-rebuild (timeout, 5xx), the rebuild does **not** silently switch to NoopJudge. Remaining candidates queue to `findings_pending_judge` and are surfaced in `/v1/health` as a count.

---

## 9. Category catalogue (one row per finding kind)

This is the canonical list for MVP. Adding categories beyond MVP requires a new design doc that updates this table.

| Category | Severity | Layer | Slice | Real-data fixture | Zero-emit fixture |
|---|---|---|---|---|---|
| `missing_verification` | medium | L1 | slice-14 | `aac68973` (≥1 expected) | one slice-1 fixture session that passes all tests |
| `tool_failure` | high | L1 | slice-14 | `aac68973` (≥1 expected — known Bash failures) | a session with zero `is_error == true` |
| `risky_action` | high | L1+L2 | slice-16 | a curated transcript with a synthetic `rm -rf` Bash | a read-only browsing session |
| `context_bloat` | low | L1+L2 | slice-16 | a transcript with a known >50 KB grep output | a transcript whose largest output is <2 KB |
| `final_state_mismatch` | medium | L1+L2 | slice-16 | a session where user asks "make tests pass" and final state has failing tests | a session that closes with green verification |

---

## 10. Provenance & schema versioning

Every stored `Finding` carries:

```json
{
  "finding_id": "find_…",
  "schema_version": "finding.v1",
  "session_id": "sess_…",
  "category": "missing_verification",
  "severity": "medium",
  "confidence": 0.9,
  "evidence_refs": ["ev_…", "ev_…"],
  "summary": "Action episode … had no following verification.",
  "provenance": {
    "extractor": "missing_verification.v1",
    "layer": "L1",
    "judge": null,
    "judge_template_version": null,
    "rule_pack": null
  },
  "created_at": "...",
  "evidence_projection": { ... }   // identical to what the judge saw, when L2; otherwise the L1-side projection
}
```

`extractor` and `judge_template_version` are stamped at write time. Re-running the engine after bumping a version creates new Finding rows (the old ones are kept until retention sweep — slice-19 — handles them). This guarantees the data plane stays auditable through engine evolution.

---

## 11. Failure modes and degrade behaviour

| Failure | Engine response | User-visible signal |
|---|---|---|
| L1 extractor panics on a session | Skip that extractor for that session; log + emit a `parser_error`-shaped finding. Other extractors continue. | `/v1/health` reports `insight_engine_errors_24h`. |
| L2 judge HTTP error | Queue remaining candidates as `findings_pending_judge`. Do **not** silently drop. | `/v1/health.judge_pending_count` non-zero. |
| Budget exhausted | Same as judge HTTP error. | Same. |
| Cache table corruption | Cache miss treated as fresh call. | None. |
| `evidence_refs` references deleted event | Finding is excluded from `/v1/findings` results but kept in DB until retention sweep. | None until retention sweep emits an audit row. |

---

## 12. Hand-off table to slice docs

The slice-14/15/16 design docs and TDD plans extend this architecture concretely:

| File | Concretely lands |
|---|---|
| `2026-05-27-witmcc-slice14-insight-l1-design.md` | `MissingVerification`, `ToolFailure` extractors + `Finding` table + `/v1/findings*` Pull API. **No judge.** |
| `2026-05-27-witmcc-slice15-insight-l2-infra-design.md` | `JudgeProvider` trait + `AnthropicJudge` + `NoopJudge` + `judge_verdict_cache` + `findings_pending_judge` + `--judge` CLI + `/v1/health.judge_*` counters. **No new categories.** |
| `2026-05-27-witmcc-slice16-insight-l2-categories-design.md` | `RiskyAction`, `ContextBloat`, `FinalStateMismatch` extractors + per-category projection types. |

After slice-16 lands, M5 closes; AC-4 is fully covered.

---

## 13. Out of scope (do not slip into MVP)

- Cross-session pattern detection.
- Patterns as a first-class object (`Pattern` schema in `docs/03_data_model_spec.html`) — they appear in data-model spec but no MVP slice ships them.
- Self-training of confidence floors (we do not learn floors from past judge verdicts; they are constants).
- Multi-provider judge support (only Anthropic in MVP).
- Streaming judge calls (sequential, non-streaming, structured-output only).
- Judge invocation outside `rebuild_session` (no view-time judging).
