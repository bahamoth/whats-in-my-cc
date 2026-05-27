# Slice-15 Design — Insight Engine L2 Infrastructure

**Date:** 2026-05-27
**Branch (to be cut):** `slice15-insight-l2-infra` off slice-14 merge.
**Goal:** Land the LLM judge layer infrastructure — `JudgeProvider` trait, three implementations (`AnthropicJudge`, `NoopJudge`, `FixtureJudge`), the `judge_verdict_cache` and `findings_pending_judge` tables, CLI flags (`--judge`, `--judge-budget`), and `/v1/health.judge_*` counters. **No new finding categories.**

This is **pure infrastructure** — no user-visible findings change between slice-14 and slice-15. Slice-16 ships categories that consume this infrastructure.

---

## 1. Motivation

Per the Insight Engine Architecture spec (`2026-05-27-witmcc-insight-engine-architecture.md`) §5, the L2 judge is **off by default**, **budget-capped**, **cache-keyed**. Slice-15 lands all three concerns without yet introducing a category that depends on them. The benefit of this split:

- The judge surface (trait + cache + budget + plumbing into pipeline) is non-trivial; landing it on top of stable L1 findings keeps the diff reviewable.
- Slice-16 only has to add three new extractor files plus their EvidenceProjection types; no new infrastructure code, no new migration.
- An exit gate: if the cost model in production looks worse than the architecture's estimate (§8), slice-15 can ship with `AnthropicJudge` disabled by default and we revisit before slice-16.

---

## 2. Scope

### In scope

- `JudgeProvider` trait per architecture spec §5.
- Three implementations:
  - `AnthropicJudge` — real Anthropic SDK call, with prompt caching on stable header (system + category schema).
  - `NoopJudge` — returns `{ promote: false, reason: "judge_disabled" }` for every input.
  - `FixtureJudge` — test-only; reads `(category, evidence_hash) → JudgeVerdict` from a JSON file.
- New table `judge_verdict_cache` (migration `0009_judge_cache.sql`).
- New table `findings_pending_judge` (migration `0010_findings_pending.sql`).
- Pipeline extension: after L1 candidates are extracted, candidates whose `promotion_policy()` is `Never` or `IfAbove(threshold)` with confidence below threshold are routed to the judge.
- CLI flags on `witmcc serve`:
  - `--judge {none|anthropic|fixture}` (default `none` → `NoopJudge`).
  - `--judge-budget N` (default 20; ignored when `--judge=none`).
  - `--judge-fixture-path <path>` (required when `--judge=fixture`).
- `/v1/health` extension: `judge_calls_24h`, `judge_pending_count`, `judge_cache_hits_24h`, `judge_cache_misses_24h`, `judge_budget_exhaustions_24h`.
- A `noop_category` test-only category that exercises the full L2 path (cache, budget, pending queue) without producing user-visible findings. Gated behind `cfg(test)` so production never sees it.

### Out of scope

- New categories that ride on the judge (slice-16).
- Streaming judge calls.
- Parallel judge calls.
- Judge-call tracing in OTel (post-MVP).

---

## 3. `JudgeProvider` trait + types

```rust
// src/insight/judge/mod.rs
pub mod cache;
pub mod budget;
pub mod providers;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct JudgePrompt {
    pub category: String,
    pub candidate_id: String,         // == finding_id derivation key
    pub evidence_projection: serde_json::Value,
    pub system_template: String,      // versioned with prompt_template_version
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct JudgeVerdict {
    pub promote: bool,
    pub confidence_l2: f32,
    pub reason: String,
    pub mismatch_summary: Option<String>,  // populated for final_state_mismatch
}

#[async_trait::async_trait]
pub trait JudgeProvider: Send + Sync {
    async fn judge(&self, prompt: JudgePrompt) -> Result<JudgeVerdict, JudgeError>;
    fn model_id(&self) -> &'static str;
    fn prompt_template_version(&self) -> &'static str;
}

pub enum JudgeError {
    Transport(String),  // network failure / 5xx
    Schema(String),     // model returned malformed JSON
    Timeout,
    BudgetExhausted,
    Disabled,
}
```

`JudgeError::BudgetExhausted` is **not** returned by the provider itself — it is returned by the budget guard wrapping the provider (see §4).

### `AnthropicJudge`

```rust
pub struct AnthropicJudge {
    client: AnthropicClient,
    model: &'static str,
}

impl AnthropicJudge {
    pub const MODEL: &'static str = "claude-sonnet-4-6";    // pin to current
    pub const PROMPT_TEMPLATE_VERSION: &'static str = "judge@v1";

    pub fn from_env() -> Result<Self> {
        let key = std::env::var("ANTHROPIC_API_KEY")
            .map_err(|_| anyhow!("ANTHROPIC_API_KEY not set"))?;
        Ok(Self { client: AnthropicClient::new(&key)?, model: Self::MODEL })
    }
}

#[async_trait::async_trait]
impl JudgeProvider for AnthropicJudge {
    async fn judge(&self, p: JudgePrompt) -> Result<JudgeVerdict, JudgeError> {
        // 1. Build messages: [
        //      { role: "system", content: [cache_control + p.system_template + schema] },
        //      { role: "user",   content: serde_json::to_string(&p.evidence_projection) }
        //    ]
        // 2. Request structured output (JSON tool use) with JudgeVerdict schema.
        // 3. Parse + return; map any error to JudgeError variants.
    }
    fn model_id(&self) -> &'static str { Self::MODEL }
    fn prompt_template_version(&self) -> &'static str { Self::PROMPT_TEMPLATE_VERSION }
}
```

`AnthropicClient` is a thin wrapper around `reqwest` calling the public API (per `claude-api` skill conventions). We do not add the official SDK crate; the surface we need is small (one request shape, structured output).

### `NoopJudge`

```rust
pub struct NoopJudge;

#[async_trait::async_trait]
impl JudgeProvider for NoopJudge {
    async fn judge(&self, _p: JudgePrompt) -> Result<JudgeVerdict, JudgeError> {
        Err(JudgeError::Disabled)
    }
    fn model_id(&self) -> &'static str { "noop" }
    fn prompt_template_version(&self) -> &'static str { "noop" }
}
```

Returning `Err(Disabled)` (not `Ok(promote=false)`) so the pipeline can distinguish "judge disabled" from "judge ran and said no".

### `FixtureJudge`

```rust
pub struct FixtureJudge {
    table: HashMap<(String, String), JudgeVerdict>,
    misses_policy: MissesPolicy,
}

pub enum MissesPolicy { Error, ReturnPromoteFalse }

impl FixtureJudge {
    pub fn load(path: &Path) -> Result<Self> {
        let raw: HashMap<String, JudgeVerdict> = serde_json::from_reader(File::open(path)?)?;
        // key is "category||evidence_hash"
        let table = raw.into_iter().map(|(k, v)| {
            let mut parts = k.splitn(2, "||");
            ((parts.next().unwrap().to_string(), parts.next().unwrap().to_string()), v)
        }).collect();
        Ok(Self { table, misses_policy: MissesPolicy::Error })
    }
}
```

Used only by `cfg(test)` and by smoke-test scenarios where we want deterministic judge outcomes without LLM cost.

---

## 4. Budget guard

```rust
// src/insight/judge/budget.rs
pub struct BudgetGuard<P: JudgeProvider> {
    inner: P,
    remaining: AtomicUsize,
}

impl<P: JudgeProvider> BudgetGuard<P> {
    pub fn new(inner: P, budget: usize) -> Self {
        Self { inner, remaining: AtomicUsize::new(budget) }
    }
}

#[async_trait::async_trait]
impl<P: JudgeProvider> JudgeProvider for BudgetGuard<P> {
    async fn judge(&self, p: JudgePrompt) -> Result<JudgeVerdict, JudgeError> {
        if self.remaining.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |r| r.checked_sub(1)).is_err() {
            return Err(JudgeError::BudgetExhausted);
        }
        self.inner.judge(p).await
    }
    fn model_id(&self) -> &'static str { self.inner.model_id() }
    fn prompt_template_version(&self) -> &'static str { self.inner.prompt_template_version() }
}
```

Budget is **per-`rebuild_session`-invocation**, not global. Built fresh in each invocation. Architecture spec §5 detail.

---

## 5. Cache

### Schema

```sql
-- migrations/20260531120000_0009_judge_cache.sql
CREATE TABLE IF NOT EXISTS judge_verdict_cache (
    cache_key                   TEXT PRIMARY KEY,
    category                    TEXT NOT NULL,
    model_id                    TEXT NOT NULL,
    prompt_template_version     TEXT NOT NULL,
    evidence_hash               TEXT NOT NULL,
    verdict_json                TEXT NOT NULL,        -- JSON-serialised JudgeVerdict
    created_at                  TEXT NOT NULL DEFAULT (datetime('now')),
    last_hit_at                 TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_judge_cache_category ON judge_verdict_cache(category);
```

### Key derivation

```rust
pub fn cache_key(p: &JudgePrompt, prov: &dyn JudgeProvider) -> String {
    let bytes = format!(
        "{}\0{}\0{}\0{}",
        p.category,
        prov.model_id(),
        prov.prompt_template_version(),
        evidence_hash(&p.evidence_projection)
    );
    format!("sha256:{}", sha256_hex(bytes.as_bytes()))
}

fn evidence_hash(proj: &serde_json::Value) -> String {
    // Canonical JSON (sort keys recursively) then sha256
    let canon = canonical_json(proj);
    sha256_hex(canon.as_bytes())
}
```

### Wrapper

```rust
pub struct CachedProvider<P: JudgeProvider> {
    inner: P,
    pool: SqlitePool,
}

#[async_trait::async_trait]
impl<P: JudgeProvider> JudgeProvider for CachedProvider<P> {
    async fn judge(&self, p: JudgePrompt) -> Result<JudgeVerdict, JudgeError> {
        let key = cache_key(&p, &self.inner);
        if let Some(v) = cache::get(&self.pool, &key).await? {
            metrics::judge_cache_hit();
            cache::touch(&self.pool, &key).await?;
            return Ok(v);
        }
        metrics::judge_cache_miss();
        let v = self.inner.judge(p.clone()).await?;
        cache::put(&self.pool, &key, &p, &v, &self.inner).await?;
        Ok(v)
    }
    fn model_id(&self) -> &'static str { self.inner.model_id() }
    fn prompt_template_version(&self) -> &'static str { self.inner.prompt_template_version() }
}
```

### Composition

The runtime composes `BudgetGuard<CachedProvider<AnthropicJudge>>`. Cache wraps the network; budget wraps the cache. Order matters — putting budget inside the cache would charge against the budget on cache hits, which we don't want.

---

## 6. `findings_pending_judge` queue

### Schema

```sql
-- migrations/20260531180000_0010_findings_pending.sql
CREATE TABLE IF NOT EXISTS findings_pending_judge (
    candidate_id        TEXT PRIMARY KEY,                  -- == finding_id derivation
    schema_version      TEXT NOT NULL DEFAULT 'pending_finding.v1',
    session_id          TEXT NOT NULL,
    category            TEXT NOT NULL,
    confidence_l1       REAL NOT NULL,
    evidence_refs       TEXT NOT NULL,
    evidence_projection TEXT NOT NULL,
    queued_at           TEXT NOT NULL DEFAULT (datetime('now')),
    last_attempt_at     TEXT,
    attempts            INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_pending_session ON findings_pending_judge(session_id);
```

### Flow

1. Pipeline extracts L1 candidates.
2. For each candidate whose category requires the judge:
   a. Build the projection.
   b. Compute cache key + lookup.
   c. On cache hit, treat as judge call (skip budget).
   d. On cache miss, attempt judge call through budget guard.
      - On `Ok(verdict)`, promote or discard per `verdict.promote`.
      - On `BudgetExhausted` or `Transport` error, **queue** to `findings_pending_judge`.
3. At the start of the next rebuild, the pipeline first drains pending entries (respecting the same budget). If still budget-exhausted, they remain queued.

This guarantees monotone progress and never silently drops a candidate.

### `/v1/findings?status=pending`

Returns the rows in `findings_pending_judge` materialised into the same response shape as a normal finding, with `status = "pending_judge"` and `provenance.judge = null` (judge hasn't run yet).

---

## 7. CLI surface

```bash
witmcc serve --judge none           # default
witmcc serve --judge anthropic --judge-budget 20
witmcc serve --judge fixture --judge-fixture-path tests/fixtures/judge/scenario_a.json
```

### Behaviour

- `--judge none` ⇒ pipeline composes `BudgetGuard<NoopJudge>` (budget irrelevant; NoopJudge errors with `Disabled`). Categories that need the judge always queue to pending. Slice-14 categories (`missing_verification`, `tool_failure`) are unaffected.
- `--judge anthropic` ⇒ requires `ANTHROPIC_API_KEY`. Composes `BudgetGuard<CachedProvider<AnthropicJudge>>`.
- `--judge fixture --judge-fixture-path <path>` ⇒ composes `BudgetGuard<CachedProvider<FixtureJudge>>`. Used by smoke tests + CI.
- Logged on boot: `LLM judge: <kind> (budget <N>)`. Always to stderr so it can be tee'd to a log.

---

## 8. `/v1/health` extension

Current `/v1/health` returns `{ status, build_sha }`. Slice-15 adds:

```json
{
  "status": "ok",
  "build_sha": "...",
  "insight": {
    "judge_kind": "noop",
    "judge_calls_24h": 0,
    "judge_pending_count": 12,
    "judge_cache_hits_24h": 0,
    "judge_cache_misses_24h": 0,
    "judge_budget_exhaustions_24h": 0
  }
}
```

`judge_pending_count` is a direct SQL `SELECT COUNT(*) FROM findings_pending_judge`. The `_24h` counters require an in-memory ring buffer or a small audit table; slice-15 uses an **in-memory atomic counter** (resets on serve restart). Persistence across restarts is post-MVP.

---

## 9. Prompt template

```
SYSTEM:
You are an analysis judge for a software-engineering observation tool. You are given:
- A finding category (one of: risky_action, context_bloat, final_state_mismatch).
- The structured evidence the deterministic extractor used to identify a candidate.

Decide whether the candidate should be promoted to a stored finding.

Return JSON of this exact shape (no prose, no markdown):

{
  "promote": boolean,
  "confidence_l2": number between 0.0 and 1.0,
  "reason": "<one short sentence>",
  "mismatch_summary": null | "<one paragraph when category == 'final_state_mismatch'>"
}

Rules:
- Promote only if the evidence shows a real problem; do not promote based on what is missing from the evidence.
- Set confidence_l2 = 0.0 if you would not promote.
- "reason" must reference at least one field name from the evidence.
- "mismatch_summary" is required when category == 'final_state_mismatch' and promote == true; null otherwise.

USER:
<evidence_projection JSON>
```

The template is stored as a const string in `src/insight/judge/prompts/judge_v1.txt`. The constant's SHA-256 is part of the `prompt_template_version` derivation (`"judge@v1#{first 12 hex of sha256}"`). Any edit to the template auto-bumps the version, invalidating cache entries without manual intervention.

---

## 10. Failure modes

| Failure | Behaviour |
|---|---|
| ANTHROPIC_API_KEY missing when `--judge anthropic` | `witmcc serve` exits at boot with `1` and a clear message. |
| Anthropic API returns 5xx | `JudgeError::Transport`. Candidate queued to `findings_pending_judge`. |
| Anthropic returns malformed JSON | `JudgeError::Schema`. Same as transport: queue. Audit row written. |
| Budget exhausted mid-rebuild | All remaining judge-required candidates queued. Health counter `judge_budget_exhaustions_24h` ++. |
| Fixture file missing entry for a (category, hash) | `FixtureJudge.misses_policy = Error` raises; tests catch this. |
| Cache table grows unbounded | Retention sweep (slice-19) prunes entries older than 30 days with `last_hit_at` outside that window. |

---

## 11. Deviations index (slice-15)

| ID | Description |
|---|---|
| DEV-S15-01 | Slice-15 introduces **no new finding categories**. The infrastructure is exercised only by the test-only `noop_category` (cfg(test)). |
| DEV-S15-02 | `BudgetGuard` is per-rebuild, not global. Two concurrent rebuilds (out-of-scope but allowed in tests) each get their own budget. |
| DEV-S15-03 | `_24h` health counters are in-memory; they reset on server restart. Persistent counters are post-MVP. |
| DEV-S15-04 | Prompt template version embeds a content hash so accidental template edits do not silently reuse stale cache. |
| DEV-S15-05 | We hand-roll the Anthropic client over `reqwest` rather than depending on a Rust SDK crate. Reason: the surface we need is small (one structured-output call) and adding a dependency for it is disproportionate. |
| DEV-S15-06 | `JudgeError::Disabled` from NoopJudge is treated identically to `BudgetExhausted` from the pipeline's perspective: the candidate is queued. This keeps the "I want to see what would queue without spending money" workflow easy: run with `--judge none --judge-budget 20` and inspect `findings_pending_judge`. |
| DEV-S15-07 | Cache key includes `prompt_template_version`, not just `model_id` + `evidence_hash`. Changing the prompt template invalidates the cache for that category, even if model and evidence are unchanged. |

---

## 12. Commit plan summary

See `2026-05-27-witmcc-slice15-insight-l2-infra.md`. Seven commits:

1. `test(slice-15): red-locking tests for JudgeProvider + cache + budget + pending queue`
2. `feat(db): 0009_judge_cache + 0010_findings_pending migrations + repos`
3. `feat(insight): JudgeProvider trait + JudgePrompt + JudgeVerdict types`
4. `feat(insight): NoopJudge + FixtureJudge implementations`
5. `feat(insight): AnthropicJudge implementation + prompt template`
6. `feat(insight): CachedProvider + BudgetGuard composition`
7. `feat(cli): --judge / --judge-budget / --judge-fixture-path + pipeline integration + /v1/health.insight.*`
