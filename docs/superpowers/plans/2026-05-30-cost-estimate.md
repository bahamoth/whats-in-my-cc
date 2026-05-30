# Cost Estimate (public-pricing 추정) — Implementation Plan (Slice 5 of insight-surface-redesign)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Surface a per-session **estimated** dollar cost (Q2, spec §6.5 / §11.3) derived from the existing `usage_facet` per-model token sums × a small public-pricing table. The value is badged **추정** (estimate), never presented as actual billing, and is designed to be superseded by the OTel `claude_code.cost.usage` metric if/when metric events arrive. We extend `GET /v1/sessions/:id/usage` (the endpoint added in slice 1) with `estimated_cost_usd` + a `cost_basis="estimate_public_pricing"` marker, plus per-model token detail so the breakdown can drill.

**Architecture:** A pure, deterministic cost function (`src/insight/pricing.rs`) holds a hardcoded **public-rate ESTIMATE** pricing table (per-model `$/Mtoken` for input / cache_creation / cache_read / output) with the source + date documented in a comment, and computes `f64` USD from per-model token counts. Unknown models (no pricing entry) contribute **$0** and are flagged in a `models_without_pricing` list so the UI can disclose incomplete coverage rather than silently undercount. The function is fed by the *existing* `usage_facet` rollup, which we first extend so its per-model rows carry the full token split (today `ModelUsage` only carries `output_tokens`; cost needs input + cache_creation + cache_read too). The handler in `src/api/routes.rs` calls the pure function and serializes the result onto the existing `SessionUsageDto`. No new table, no migration — this slice is pure read-path computation over slice-1 data.

**Why no actual billing:** cache_read is billed at a steep discount, cache_creation at a premium, and *all* published rates drift (and differ by service tier / negotiated contract). Per CLAUDE.md "Real-data anchoring" and spec §11.3, we commit to a clearly-labelled estimate only; the code comment, the `cost_basis` field, and the 추정 badge all state this. The OTel `claude_code.cost.usage` parser already exists in `src/ingest/otel_metrics.rs` but no metric events arrive for transcript-ingested sessions (spec §6.5), so it cannot be used yet.

**Tech Stack:** Rust (sqlx + SQLite, serde_json), axum (Pull API), React + TypeScript + @tanstack/react-query (frontend consumption). Tests: `cargo test`, `npx vitest run`, `npx tsc -b`.

**Real-data anchoring (read before coding):** The frozen fixture `tests/fixtures/transcripts/real/verification_v01.jsonl` is session `aac68973-729e-4014-a02b-28a556f5ff29` and its 3 assistant lines carry model **`claude-opus-4-7`** with real `usage` objects (`cache_read_input_tokens` ~187K–306K, small `input_tokens`/`output_tokens`). The pricing table MUST therefore include `claude-opus-4-7` for the real-fixture endpoint test to see `estimated_cost_usd > 0`. The dev-DB model ids called out by the redesign (`claude-opus-4-8`, `claude-haiku-4-5-20251001`, `claude-sonnet-4-6`) are also seeded so re-ingested dev sessions get a non-zero estimate. (Verified 2026-05-30 by `grep -o '"model":"[^"]*"' tests/fixtures/transcripts/real/*.jsonl` → only `claude-opus-4-7` is present in frozen fixtures.)

**Out of scope for this plan (later / other slices):** the actual KpiStrip 비용 card UI + 추정 badge rendering (frontend surface lands incrementally with the strip rework); cross-session cost baseline (proposal A, slice 6); reading the OTel `cost.usage` metric (it does not arrive for these sessions). This slice delivers the per-model token split + pure cost function + endpoint fields + frontend type/client wiring — testable on its own.

---

## File structure

| File | Responsibility | Create/Modify |
|---|---|---|
| `src/db/repo_usage_facet.rs` | extend `ModelUsage` + `session_aggregate` SQL with per-model input / cache_creation / cache_read token sums | Modify |
| `src/insight/pricing.rs` | public-pricing table constant + pure `estimate_session_cost` + `CostEstimate` / `ModelCost` | Create |
| `src/insight/mod.rs` | register `pub mod pricing;` | Modify |
| `src/api/dto.rs` | add cost fields to `SessionUsageDto` + per-model token/cost fields to `ModelUsageDto` + `cost_basis` | Modify |
| `src/api/routes.rs` | `session_usage` handler — call `estimate_session_cost`, serialize cost fields | Modify |
| `tests/api_usage.rs` | endpoint test: `estimated_cost_usd > 0` for the real fixture | Modify |
| `webui/src/api/types.ts` | add cost fields to `SessionUsageDto` / `ModelUsageDto` TS types | Modify |
| `webui/src/api/__tests__/client.endpoints.test.ts` | update `getSessionUsage` case with new fields | Modify |
| `docs/implementation-notes.html` | new `§` — cost estimate slice notes | Modify |

---

## Task 1: Extend per-model token split in the usage aggregate (`src/db/repo_usage_facet.rs`)

Today `ModelUsage` carries only `model`, `turns`, `output_tokens` (see `src/db/repo_usage_facet.rs` lines 44–49). Cost needs the full per-model token split (input / cache_creation / cache_read), because each token category has a different rate. We add those three fields and extend the existing `session_aggregate` `by_model` query. The existing slice-1 in-file test `roundtrip_and_aggregate` constructs rows with all token fields and reads `model`/`turns`/`output_tokens`, so adding fields with `#[derive(Default)]` does not break it — but we add fresh assertions to lock the new sums.

**Files:**
- Modify: `src/db/repo_usage_facet.rs`

- [ ] **Step 1: Write the failing assertions**

In `src/db/repo_usage_facet.rs`, inside the existing `#[cfg(test)] mod tests` block, add assertions to the existing `roundtrip_and_aggregate` test (the `opus`/`haiku` rows are built by the `row()` helper as `row("raw_001", "claude-opus-4-8", 2, 100, 5000, 300)` = input 2, cache_creation 100, cache_read 5000, output 300). Append immediately after the existing `assert_eq!(opus.output_tokens, 300);` line:

```rust
        assert_eq!(opus.input_tokens, 2, "per-model input sum");
        assert_eq!(opus.cache_creation_input_tokens, 100, "per-model cache_creation sum");
        assert_eq!(opus.cache_read_input_tokens, 5000, "per-model cache_read sum");
```

And after the existing `assert_eq!(haiku.output_tokens, 400);` line:

```rust
        assert_eq!(haiku.input_tokens, 3);
        assert_eq!(haiku.cache_creation_input_tokens, 200);
        assert_eq!(haiku.cache_read_input_tokens, 6000);
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib repo_usage_facet 2>&1 | tail -20`
Expected: FAIL — `ModelUsage` has no field `input_tokens` (compile error E0609).

- [ ] **Step 3: Extend the `ModelUsage` struct**

In `src/db/repo_usage_facet.rs`, replace the existing struct (lines 44–49):

```rust
#[derive(Debug, Clone, Default)]
pub struct ModelUsage {
    pub model: String,
    pub turns: i64,
    pub output_tokens: i64,
}
```

with:

```rust
#[derive(Debug, Clone, Default)]
pub struct ModelUsage {
    pub model: String,
    pub turns: i64,
    pub input_tokens: i64,
    pub cache_creation_input_tokens: i64,
    pub cache_read_input_tokens: i64,
    pub output_tokens: i64,
}
```

- [ ] **Step 4: Extend the `by_model` SQL + row mapper**

In `session_aggregate`, replace the `by_model_rows` query (the `SELECT COALESCE(model,'unknown') ...` block) with:

```rust
    let by_model_rows = sqlx::query(
        "SELECT COALESCE(model,'unknown') AS model,
                COUNT(*) AS turns,
                COALESCE(SUM(input_tokens),0) AS input_tokens,
                COALESCE(SUM(cache_creation_input_tokens),0) AS cc,
                COALESCE(SUM(cache_read_input_tokens),0) AS cr,
                COALESCE(SUM(output_tokens),0) AS output_tokens
         FROM usage_facet WHERE session_id = ?
         GROUP BY model ORDER BY turns DESC",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?;
```

Then replace the `map_model_usage` helper (the `fn map_model_usage(r: sqlx::sqlite::SqliteRow) -> ModelUsage { ... }` block):

```rust
fn map_model_usage(r: sqlx::sqlite::SqliteRow) -> ModelUsage {
    ModelUsage {
        model: r.get("model"),
        turns: r.get::<i64, _>("turns"),
        input_tokens: r.get::<i64, _>("input_tokens"),
        cache_creation_input_tokens: r.get::<i64, _>("cc"),
        cache_read_input_tokens: r.get::<i64, _>("cr"),
        output_tokens: r.get::<i64, _>("output_tokens"),
    }
}
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test --lib repo_usage_facet 2>&1 | tail -20`
Expected: PASS (`roundtrip_and_aggregate` + `insert_or_replace_dedup`).

- [ ] **Step 6: Commit**

```bash
git add src/db/repo_usage_facet.rs
git commit -m "feat(cost): per-model input/cache token split in usage aggregate"
```

---

## Task 2: Pure pricing table + cost function (`src/insight/pricing.rs`)

A self-contained, deterministic module. The pricing table is an **estimate** of public per-Mtoken rates; the source + date + drift caveat live in a header comment. The pure function maps `&[ModelUsage]` → a `CostEstimate` (total USD + per-model breakdown + unknown-model flags). Unknown models contribute $0 and are recorded so the caller can disclose incomplete coverage.

**Files:**
- Create: `src/insight/pricing.rs`
- Modify: `src/insight/mod.rs` (add `pub mod pricing;`)
- Test: in-file `#[cfg(test)]` module

- [ ] **Step 1: Write the failing test + stubs**

Create `src/insight/pricing.rs` with the table, types, stub, and tests:

```rust
//! Cost ESTIMATE (Q2, spec §6.5 / §11.3) — public-pricing approximation.
//!
//! ⚠️  THIS IS NOT ACTUAL BILLING. The numbers below are a hardcoded estimate
//! of *public* per-million-token list rates. Real cost differs by service
//! tier, negotiated contract, promotional discounts, and rate changes over
//! time. We surface this only as `cost_basis = "estimate_public_pricing"`,
//! badged 추정 in the UI, and replace it with the OTel
//! `claude_code.cost.usage` metric if/when those metric events arrive
//! (`src/ingest/otel_metrics.rs` already parses that instrument; no metric
//! events currently arrive for transcript-ingested sessions — spec §6.5).
//!
//! Rates are US dollars per 1,000,000 tokens (1 Mtoken).
//! Source: Anthropic public pricing page (claude.com/pricing), captured
//! 2026-05-30. Values are ESTIMATES and may drift — when they change, update
//! `PRICING` and bump `PRICING_VERSION`, and re-anchor the unit test.
//!
//! cache_read is billed at a discount; cache_creation at a premium; both are
//! kept as separate line items here because they have different rates.

use crate::db::repo_usage_facet::ModelUsage;

/// Bump when the table or the estimation method changes. Surfaced as
/// provenance so a stored/exported estimate can be traced to its rate set.
pub const PRICING_VERSION: &str = "pricing_estimate@v1";

/// Marker placed on the API response so the UI shows the 추정 badge and never
/// presents this as actual billing.
pub const COST_BASIS_ESTIMATE: &str = "estimate_public_pricing";

/// Per-Mtoken USD rates for one model. All four token classes priced
/// independently. ESTIMATE only — see module header.
#[derive(Debug, Clone, Copy)]
pub struct ModelRates {
    pub input_per_mtok: f64,
    pub cache_creation_per_mtok: f64,
    pub cache_read_per_mtok: f64,
    pub output_per_mtok: f64,
}

/// Public-rate ESTIMATE table. `model` is matched against
/// `assistant_message.payload.model` / `usage_facet.model` exactly.
///
/// Includes `claude-opus-4-7` (present in the frozen real fixture) and the
/// dev-DB model ids called out in the redesign spec. Unknown models fall
/// through to $0 and are flagged (see `estimate_session_cost`).
pub const PRICING: &[(&str, ModelRates)] = &[
    // Opus-tier ESTIMATE: input $15, cache_creation $18.75 (1.25×),
    // cache_read $1.50 (0.1×), output $75 per Mtoken.
    (
        "claude-opus-4-8",
        ModelRates {
            input_per_mtok: 15.0,
            cache_creation_per_mtok: 18.75,
            cache_read_per_mtok: 1.5,
            output_per_mtok: 75.0,
        },
    ),
    (
        "claude-opus-4-7",
        ModelRates {
            input_per_mtok: 15.0,
            cache_creation_per_mtok: 18.75,
            cache_read_per_mtok: 1.5,
            output_per_mtok: 75.0,
        },
    ),
    // Sonnet-tier ESTIMATE: input $3, cache_creation $3.75, cache_read $0.30,
    // output $15 per Mtoken.
    (
        "claude-sonnet-4-6",
        ModelRates {
            input_per_mtok: 3.0,
            cache_creation_per_mtok: 3.75,
            cache_read_per_mtok: 0.3,
            output_per_mtok: 15.0,
        },
    ),
    // Haiku-tier ESTIMATE: input $1, cache_creation $1.25, cache_read $0.10,
    // output $5 per Mtoken.
    (
        "claude-haiku-4-5-20251001",
        ModelRates {
            input_per_mtok: 1.0,
            cache_creation_per_mtok: 1.25,
            cache_read_per_mtok: 0.1,
            output_per_mtok: 5.0,
        },
    ),
];

/// Per-model estimated cost (USD), with a `priced` flag so unknown models are
/// visibly $0 rather than silently dropped.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelCost {
    pub model: String,
    pub estimated_cost_usd: f64,
    pub priced: bool,
}

/// Session-level estimate. `total_usd` sums only priced models;
/// `models_without_pricing` lists models we could not price (so the UI can
/// disclose incomplete coverage).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CostEstimate {
    pub total_usd: f64,
    pub per_model: Vec<ModelCost>,
    pub models_without_pricing: Vec<String>,
}

/// Look up the ESTIMATE rates for a model id (exact match).
pub fn rates_for(model: &str) -> Option<ModelRates> {
    PRICING.iter().find(|(m, _)| *m == model).map(|(_, r)| *r)
}

/// Compute the public-pricing ESTIMATE for a session from its per-model token
/// sums. Unknown models contribute $0 and are flagged. Pure + deterministic.
pub fn estimate_session_cost(by_model: &[ModelUsage]) -> CostEstimate {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mu(model: &str, input: i64, cc: i64, cr: i64, output: i64) -> ModelUsage {
        ModelUsage {
            model: model.into(),
            turns: 1,
            input_tokens: input,
            cache_creation_input_tokens: cc,
            cache_read_input_tokens: cr,
            output_tokens: output,
        }
    }

    #[test]
    fn deterministic_cost_for_known_model() {
        // 1,000,000 of each class for opus-4-8: 15 + 18.75 + 1.5 + 75 = 110.25
        let est = estimate_session_cost(&[mu("claude-opus-4-8", 1_000_000, 1_000_000, 1_000_000, 1_000_000)]);
        assert!((est.total_usd - 110.25).abs() < 1e-9, "got {}", est.total_usd);
        assert_eq!(est.per_model.len(), 1);
        assert!(est.per_model[0].priced);
        assert!(est.models_without_pricing.is_empty());
    }

    #[test]
    fn cache_read_is_cheap_relative_to_output() {
        // 1M cache_read on opus = $1.50; 1M output = $75. Locks the rate split.
        let read = estimate_session_cost(&[mu("claude-opus-4-8", 0, 0, 1_000_000, 0)]);
        let out = estimate_session_cost(&[mu("claude-opus-4-8", 0, 0, 0, 1_000_000)]);
        assert!((read.total_usd - 1.5).abs() < 1e-9, "cache_read got {}", read.total_usd);
        assert!((out.total_usd - 75.0).abs() < 1e-9, "output got {}", out.total_usd);
        assert!(read.total_usd < out.total_usd);
    }

    #[test]
    fn unknown_model_contributes_zero_and_is_flagged() {
        let est = estimate_session_cost(&[mu("some-future-model-x", 1_000_000, 0, 0, 1_000_000)]);
        assert_eq!(est.total_usd, 0.0);
        assert_eq!(est.models_without_pricing, vec!["some-future-model-x".to_string()]);
        assert_eq!(est.per_model.len(), 1);
        assert!(!est.per_model[0].priced);
        assert_eq!(est.per_model[0].estimated_cost_usd, 0.0);
    }

    #[test]
    fn mixed_models_sum_only_priced() {
        let est = estimate_session_cost(&[
            mu("claude-opus-4-8", 0, 0, 0, 1_000_000),   // $75
            mu("claude-haiku-4-5-20251001", 0, 0, 0, 1_000_000), // $5
            mu("unknown-y", 0, 0, 0, 1_000_000),         // $0, flagged
        ]);
        assert!((est.total_usd - 80.0).abs() < 1e-9, "got {}", est.total_usd);
        assert_eq!(est.models_without_pricing, vec!["unknown-y".to_string()]);
        assert_eq!(est.per_model.len(), 3);
    }

    #[test]
    fn empty_input_is_zero() {
        let est = estimate_session_cost(&[]);
        assert_eq!(est.total_usd, 0.0);
        assert!(est.per_model.is_empty());
        assert!(est.models_without_pricing.is_empty());
    }

    #[test]
    fn fixture_model_opus_4_7_is_priced() {
        // Real fixture verification_v01.jsonl carries claude-opus-4-7 — it MUST
        // be in the table or the endpoint test would see $0.
        assert!(rates_for("claude-opus-4-7").is_some());
    }
}
```

Add to `src/insight/mod.rs`: `pub mod pricing;` (keep the list alphabetical-ish — insert after `pub mod judge;` or wherever it reads cleanly).

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib insight::pricing 2>&1 | tail -20`
Expected: FAIL — panics at `todo!()` in `estimate_session_cost` (the `rates_for` / table tests may compile but the estimate tests panic).

- [ ] **Step 3: Implement `estimate_session_cost`**

Replace the `todo!()` body:

```rust
pub fn estimate_session_cost(by_model: &[ModelUsage]) -> CostEstimate {
    let mut out = CostEstimate::default();
    for m in by_model {
        match rates_for(&m.model) {
            Some(r) => {
                let usd = (m.input_tokens as f64) * r.input_per_mtok / 1_000_000.0
                    + (m.cache_creation_input_tokens as f64) * r.cache_creation_per_mtok / 1_000_000.0
                    + (m.cache_read_input_tokens as f64) * r.cache_read_per_mtok / 1_000_000.0
                    + (m.output_tokens as f64) * r.output_per_mtok / 1_000_000.0;
                out.total_usd += usd;
                out.per_model.push(ModelCost {
                    model: m.model.clone(),
                    estimated_cost_usd: usd,
                    priced: true,
                });
            }
            None => {
                out.models_without_pricing.push(m.model.clone());
                out.per_model.push(ModelCost {
                    model: m.model.clone(),
                    estimated_cost_usd: 0.0,
                    priced: false,
                });
            }
        }
    }
    out
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib insight::pricing 2>&1 | tail -20`
Expected: PASS (6 tests).

- [ ] **Step 5: Commit**

```bash
git add src/insight/pricing.rs src/insight/mod.rs
git commit -m "feat(cost): public-pricing estimate table + pure estimate_session_cost"
```

---

## Task 3: Wire cost into the usage endpoint (`src/api/dto.rs`, `src/api/routes.rs`, `tests/api_usage.rs`)

Extend the slice-1 `SessionUsageDto` with `estimated_cost_usd`, `cost_basis`, `pricing_version`, `models_without_pricing`, and add per-model token + cost detail to `ModelUsageDto`. The handler at `src/api/routes.rs:382` (`session_usage`) already builds the DTO from the aggregate — we feed `agg.by_model` into `estimate_session_cost` and serialize the result. The real-fixture endpoint test then asserts `estimated_cost_usd > 0` (fixture is all `claude-opus-4-7`, which is priced).

**Files:**
- Modify: `src/api/dto.rs`, `src/api/routes.rs`
- Modify: `tests/api_usage.rs` (add a cost-assertion test)

- [ ] **Step 1: Extend the DTOs** (`src/api/dto.rs`)

Replace the existing `SessionUsageDto` and `ModelUsageDto` (currently at `src/api/dto.rs` lines 242–263) with:

```rust
/// insight-redesign #1 + #5(cost) — session token-usage aggregate, now with a
/// public-pricing **estimate** of dollar cost (Q2). `estimated_cost_usd` is
/// NOT actual billing: `cost_basis = "estimate_public_pricing"` and the UI
/// badges it 추정. Replaced by the OTel `claude_code.cost.usage` metric if/when
/// it arrives (spec §6.5).
#[derive(Serialize)]
pub struct SessionUsageDto {
    pub session_id: String,
    pub turns: i64,
    pub input_tokens: i64,
    pub cache_creation_input_tokens: i64,
    pub cache_read_input_tokens: i64,
    pub output_tokens: i64,
    /// input + cache_creation + output (cache_read is NOT billed)
    pub billed_tokens: i64,
    /// cache_read / (cache_read + cache_creation + input); null when denom 0
    pub cache_hit_ratio: Option<f64>,
    /// Estimated session cost in USD — public-pricing ESTIMATE, never actual
    /// billing. See `cost_basis` / `pricing_version`.
    pub estimated_cost_usd: f64,
    /// Always "estimate_public_pricing" for this slice — drives the 추정 badge.
    pub cost_basis: String,
    /// Rate-table version the estimate was computed against.
    pub pricing_version: String,
    /// Models in this session we could not price (excluded from the total);
    /// surfaced so the UI can disclose incomplete cost coverage.
    pub models_without_pricing: Vec<String>,
    pub by_model: Vec<ModelUsageDto>,
}

#[derive(Serialize)]
pub struct ModelUsageDto {
    pub model: String,
    pub turns: i64,
    pub input_tokens: i64,
    pub cache_creation_input_tokens: i64,
    pub cache_read_input_tokens: i64,
    pub output_tokens: i64,
    /// Per-model public-pricing ESTIMATE in USD (0 when the model is unpriced).
    pub estimated_cost_usd: f64,
    /// false when no pricing entry exists for `model` (cost is then 0).
    pub priced: bool,
}
```

- [ ] **Step 2: Write the failing endpoint assertions** (`tests/api_usage.rs`)

The existing `tests/api_usage.rs` ingests `verification_v01.jsonl` and hits `/v1/sessions/aac68973-729e-4014-a02b-28a556f5ff29/usage`. Add a second `#[tokio::test]` (reusing the same `empty_pool()` helper already in the file) that asserts the cost fields. Append to `tests/api_usage.rs`:

```rust
#[tokio::test]
async fn usage_endpoint_returns_public_pricing_estimate() {
    let pool = empty_pool().await;
    store::ingest_file(
        &pool,
        std::path::Path::new("tests/fixtures/transcripts/real/verification_v01.jsonl"),
        &NoopSink,
    )
    .await
    .unwrap();

    let state = AppState::new_for_tests(pool);
    let server = TestServer::new(router(state)).unwrap();
    let r = server
        .get("/v1/sessions/aac68973-729e-4014-a02b-28a556f5ff29/usage")
        .await;
    r.assert_status_ok();
    let body = r.json::<Value>();
    let data = &body["data"];

    // Fixture is all claude-opus-4-7 (priced) with non-zero tokens → positive.
    assert!(
        data["estimated_cost_usd"].as_f64().unwrap() > 0.0,
        "real fixture should yield a positive public-pricing estimate"
    );
    // Never presented as actual billing.
    assert_eq!(data["cost_basis"].as_str().unwrap(), "estimate_public_pricing");
    assert_eq!(data["pricing_version"].as_str().unwrap(), "pricing_estimate@v1");
    // claude-opus-4-7 is in the table → nothing unpriced for this fixture.
    assert!(data["models_without_pricing"].as_array().unwrap().is_empty());

    // Per-model detail carries the token split + per-model cost.
    let by_model = data["by_model"].as_array().unwrap();
    assert!(!by_model.is_empty());
    let m0 = &by_model[0];
    assert_eq!(m0["model"].as_str().unwrap(), "claude-opus-4-7");
    assert!(m0["priced"].as_bool().unwrap());
    assert!(m0["cache_read_input_tokens"].as_i64().unwrap() > 0);
    assert!(m0["estimated_cost_usd"].as_f64().unwrap() > 0.0);
}
```

Run: `cargo test --test api_usage 2>&1 | tail -25`
Expected: FAIL — `data["estimated_cost_usd"]` is `null` (field not serialized yet) → `.as_f64().unwrap()` panics; and `m0["priced"]` missing.

- [ ] **Step 3: Wire the handler** (`src/api/routes.rs`)

The `session_usage` handler is at `src/api/routes.rs:382`. Replace its body (the `let data = SessionUsageDto { ... };` block and the per-model `.map(...)`) so it computes the estimate from `agg.by_model` BEFORE moving it into the DTO mapper. Replace the whole `let data = SessionUsageDto { ... };` statement with:

```rust
    let cost = crate::insight::pricing::estimate_session_cost(&agg.by_model);
    let priced: std::collections::HashMap<&str, f64> = cost
        .per_model
        .iter()
        .map(|c| (c.model.as_str(), c.estimated_cost_usd))
        .collect();
    let data = SessionUsageDto {
        session_id: id,
        turns: agg.turns,
        input_tokens: agg.input_tokens,
        cache_creation_input_tokens: agg.cache_creation_input_tokens,
        cache_read_input_tokens: agg.cache_read_input_tokens,
        output_tokens: agg.output_tokens,
        billed_tokens: billed,
        cache_hit_ratio,
        estimated_cost_usd: cost.total_usd,
        cost_basis: crate::insight::pricing::COST_BASIS_ESTIMATE.to_string(),
        pricing_version: crate::insight::pricing::PRICING_VERSION.to_string(),
        models_without_pricing: cost.models_without_pricing.clone(),
        by_model: agg
            .by_model
            .into_iter()
            .map(|m| {
                let est = priced.get(m.model.as_str()).copied().unwrap_or(0.0);
                let is_priced = crate::insight::pricing::rates_for(&m.model).is_some();
                ModelUsageDto {
                    model: m.model,
                    turns: m.turns,
                    input_tokens: m.input_tokens,
                    cache_creation_input_tokens: m.cache_creation_input_tokens,
                    cache_read_input_tokens: m.cache_read_input_tokens,
                    output_tokens: m.output_tokens,
                    estimated_cost_usd: est,
                    priced: is_priced,
                }
            })
            .collect(),
    };
```

(`billed` and `cache_hit_ratio` are already computed above in the existing handler — leave those lines unchanged.) No new `use` needed since the references are fully qualified via `crate::insight::pricing::`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test api_usage 2>&1 | tail -25`
Expected: PASS (the original `usage_endpoint_returns_aggregate` plus the new cost test). Then full backend suite: `cargo test 2>&1 | tail -15` — no regressions (the slice-1 `repo_usage_facet` and `usage_facet_ingest` tests still pass).

- [ ] **Step 5: Commit**

```bash
git add src/api/dto.rs src/api/routes.rs tests/api_usage.rs
git commit -m "feat(cost): estimated_cost_usd + cost_basis on /v1/sessions/:id/usage"
```

---

## Task 4: Frontend types + client contract test

Extend the TS `SessionUsageDto` / `ModelUsageDto` to mirror the new fields, and update the existing `getSessionUsage` contract test so the client still round-trips the (now larger) envelope. No UI rendering in this slice — the 비용 card consumes these fields when the strip rework lands.

**Files:**
- Modify: `webui/src/api/types.ts`
- Modify: `webui/src/api/__tests__/client.endpoints.test.ts`

- [ ] **Step 1: Extend the TS types** (`webui/src/api/types.ts`)

Replace the existing `ModelUsageDto` (line 179) and `SessionUsageDto` (lines 181–190):

```typescript
export type ModelUsageDto = {
  model: string;
  turns: number;
  input_tokens: number;
  cache_creation_input_tokens: number;
  cache_read_input_tokens: number;
  output_tokens: number;
  /** Per-model public-pricing ESTIMATE (USD); 0 when unpriced. */
  estimated_cost_usd: number;
  priced: boolean;
};

export type SessionUsageDto = {
  session_id: string;
  turns: number;
  input_tokens: number;
  cache_creation_input_tokens: number;
  cache_read_input_tokens: number;
  output_tokens: number;
  billed_tokens: number;
  cache_hit_ratio: number | null;
  /** Public-pricing ESTIMATE (USD) — NOT actual billing. */
  estimated_cost_usd: number;
  /** "estimate_public_pricing" — drives the 추정 badge. */
  cost_basis: string;
  pricing_version: string;
  models_without_pricing: string[];
  by_model: ModelUsageDto[];
};
```

- [ ] **Step 2: Update the failing client test** (`webui/src/api/__tests__/client.endpoints.test.ts`)

The existing `getSessionUsage` case (around line 90) sends an `expected` object lacking the new fields; with the stricter type it must include them. Replace the body of that `it('getSessionUsage unwraps the usage envelope', ...)` test's `expected` literal with the full shape:

```typescript
    const expected = {
      session_id: 's1',
      turns: 3,
      input_tokens: 10,
      cache_creation_input_tokens: 20,
      cache_read_input_tokens: 900,
      output_tokens: 30,
      billed_tokens: 60,
      cache_hit_ratio: 0.96,
      estimated_cost_usd: 0.0123,
      cost_basis: 'estimate_public_pricing',
      pricing_version: 'pricing_estimate@v1',
      models_without_pricing: [],
      by_model: [
        {
          model: 'claude-opus-4-7',
          turns: 3,
          input_tokens: 10,
          cache_creation_input_tokens: 20,
          cache_read_input_tokens: 900,
          output_tokens: 30,
          estimated_cost_usd: 0.0123,
          priced: true,
        },
      ],
    };
```

(Leave the rest of the test — `fetchSpy.mockImplementation(mockJson(ENVELOPE(expected)))`, `await getSessionUsage('s1')`, `expect(out).toEqual(expected)` — unchanged.)

- [ ] **Step 3: Run tests + types**

Run from the `webui` dir context (match the project's two-server/webui convention):
`cd webui && npx vitest run src/api/__tests__/client.endpoints.test.ts 2>&1 | tail -15` → PASS
`cd webui && npx tsc -b 2>&1 | tail -15` → clean (no unused/missing-field errors)

- [ ] **Step 4: Commit**

```bash
git add webui/src/api/types.ts webui/src/api/__tests__/client.endpoints.test.ts
git commit -m "feat(cost): frontend SessionUsageDto cost fields + client contract test"
```

---

## Task 5: Rebuild DB, smoke the endpoint, document

**Files:** `docs/implementation-notes.html` (Modify)

- [ ] **Step 1: Re-ingest so dev sessions get per-model token rows**

The per-model token split is computed at query time from `usage_facet` (no migration), so existing `usage_facet` rows already suffice — but re-ingest if your dev DB predates slice 1.
Run: `cargo run --bin witmcc -- ingest --all 2>&1 | tail -5`
Expected: completes; no errors.

- [ ] **Step 2: Smoke the endpoint against a real dev session**

Run: `cargo run --bin witmcc -- serve --bind 127.0.0.1 --port 7878 &` then
`sleep 2 && curl -s http://127.0.0.1:7878/v1/sessions/653ea169-1121-442e-9cc9-776471a10895/usage | python3 -m json.tool | head -40`
Expected: JSON includes `"estimated_cost_usd"` > 0, `"cost_basis": "estimate_public_pricing"`, `"pricing_version": "pricing_estimate@v1"`, a `by_model` array where each entry has `estimated_cost_usd` + `priced`, and `models_without_pricing` listing any model id not in `PRICING` (e.g. an older opus variant). Stop the server afterward (`kill %1`).

If `models_without_pricing` is non-empty for a model you expect to be priced, that is a real signal — add the model id + its ESTIMATE rates to `PRICING` in `src/insight/pricing.rs` (each addition is locked by the deterministic unit test, per CLAUDE.md real-data anchoring), then re-run Step 2.

- [ ] **Step 3: Document in implementation-notes**

Add a new `§` section to `docs/implementation-notes.html` covering: (a) the cost value is a **public-pricing ESTIMATE**, badged 추정, `cost_basis = "estimate_public_pricing"`, never actual billing (spec §11.3); (b) the `PRICING` table location + `PRICING_VERSION` + the "update both + re-anchor the test when rates drift" rule; (c) unknown models contribute $0 and are surfaced in `models_without_pricing`; (d) this is superseded by OTel `claude_code.cost.usage` if/when metric events arrive (none do for transcript sessions today); (e) NO migration — pure read-path over slice-1 `usage_facet`. Commit:

```bash
git add docs/implementation-notes.html
git commit -m "docs(cost): implementation notes for public-pricing estimate slice"
```

---

## Done criteria

- `GET /v1/sessions/:id/usage` returns `estimated_cost_usd` (> 0 for sessions with priced models + tokens), `cost_basis = "estimate_public_pricing"`, `pricing_version`, `models_without_pricing`, and per-model `estimated_cost_usd` + `priced` + token split.
- The cost function is pure + deterministic + unit-tested against known token counts × known rates; unknown models contribute $0 and are flagged.
- Real-fixture invariant: `verification_v01.jsonl` (all `claude-opus-4-7`) yields a positive estimate via the endpoint test.
- No migration; pure read-path over slice-1 `usage_facet`.
- All new + existing tests pass; `cargo test` + (in `webui`) `npx vitest run` + `npx tsc -b` clean; no regressions.
- Code, `cost_basis`, and docs all state explicitly that this is a public-pricing **estimate**, not actual billing — to be replaced by OTel `claude_code.cost.usage` if/when present. This unblocks the redesign's Q2 비용 card (UI rendering + 추정 badge lands with the strip rework).
```