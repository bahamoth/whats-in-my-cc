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
//! Source: Anthropic public pricing page
//! (platform.claude.com/docs/en/about-claude/pricing), captured 2026-06-11.
//! Values are ESTIMATES and may drift — when they change, update `pricing.json`
//! (repo root). `scripts/update-pricing.ts` + the weekly refresh workflow
//! automate this; manual edits touch the JSON only (never re-hardcode here).
//!
//! cache_read is billed at a discount; cache_creation at a premium; both are
//! kept as separate line items here because they have different rates.

use crate::db::repo_usage_facet::ModelUsage;
use once_cell::sync::Lazy;
use serde::Deserialize;
use std::collections::BTreeMap;

/// Checked-in pricing table (repo root). Rust embeds it at compile time so the
/// runtime NEVER fetches rates externally (local-first, spec §2.4-6);
/// `scripts/update-pricing.ts` + the weekly pricing-refresh workflow keep the
/// file in sync with the public pricing page via reviewed PRs.
static PRICING_JSON: &str = include_str!("../../pricing.json");

#[derive(Debug, Deserialize)]
struct PricingFile {
    version: String,
    source_url: String,
    models: BTreeMap<String, ModelRates>,
}

static TABLE: Lazy<PricingFile> = Lazy::new(|| {
    serde_json::from_str(PRICING_JSON).expect(
        "pricing.json: schema mismatch (spec 2026-07-04 §2.1) — version/source_url/models required",
    )
});

/// Pricing-table provenance `pricing_estimate@<YYYY-MM-DD>` — the date IS the
/// version (last refresh from the public page; no arbitrary v-numbering) so a
/// stored/exported estimate's staleness is visible directly. Surfaced via the
/// usage API.
pub fn pricing_version() -> &'static str {
    &TABLE.version
}

/// Public pricing page the table was captured from (used by the refresh
/// script/workflow and surfaced for provenance).
pub fn pricing_source_url() -> &'static str {
    &TABLE.source_url
}

/// Marker placed on the API response so the UI shows the 추정 badge and never
/// presents this as actual billing.
pub const COST_BASIS_ESTIMATE: &str = "estimate_public_pricing";

/// Per-Mtoken USD rates for one model. All four token classes priced
/// independently. ESTIMATE only — see module header.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct ModelRates {
    pub input_per_mtok: f64,
    pub cache_creation_per_mtok: f64,
    pub cache_read_per_mtok: f64,
    pub output_per_mtok: f64,
}

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
    TABLE.models.get(model).copied()
}

/// Compute the public-pricing ESTIMATE for a session from its per-model token
/// sums. Unknown models contribute $0 and are flagged. Pure + deterministic.
pub fn estimate_session_cost(by_model: &[ModelUsage]) -> CostEstimate {
    let mut out = CostEstimate::default();
    for m in by_model {
        match rates_for(&m.model) {
            Some(r) => {
                let usd = (m.input_tokens as f64) * r.input_per_mtok / 1_000_000.0
                    + (m.cache_creation_input_tokens as f64) * r.cache_creation_per_mtok
                        / 1_000_000.0
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

#[cfg(test)]
mod tests {
    use super::*;

    fn mu(model: &str, input: i64, cc: i64, cr: i64, output: i64) -> ModelUsage {
        ModelUsage {
            model: model.into(),
            assistant_events: 1,
            input_tokens: input,
            cache_creation_input_tokens: cc,
            cache_read_input_tokens: cr,
            output_tokens: output,
        }
    }

    #[test]
    fn deterministic_cost_for_known_model() {
        // 1,000,000 of each class for opus-4-8: 5 + 6.25 + 0.5 + 25 = 36.75
        // (platform.claude.com/docs/en/about-claude/pricing, captured 2026-06-11)
        let est = estimate_session_cost(&[mu(
            "claude-opus-4-8",
            1_000_000,
            1_000_000,
            1_000_000,
            1_000_000,
        )]);
        assert!(
            (est.total_usd - 36.75).abs() < 1e-9,
            "got {}",
            est.total_usd
        );
        assert_eq!(est.per_model.len(), 1);
        assert!(est.per_model[0].priced);
        assert!(est.models_without_pricing.is_empty());
    }

    #[test]
    fn cache_read_is_cheap_relative_to_output() {
        // 1M cache_read on opus = $0.50; 1M output = $25. Locks the rate split.
        let read = estimate_session_cost(&[mu("claude-opus-4-8", 0, 0, 1_000_000, 0)]);
        let out = estimate_session_cost(&[mu("claude-opus-4-8", 0, 0, 0, 1_000_000)]);
        assert!(
            (read.total_usd - 0.5).abs() < 1e-9,
            "cache_read got {}",
            read.total_usd
        );
        assert!(
            (out.total_usd - 25.0).abs() < 1e-9,
            "output got {}",
            out.total_usd
        );
        assert!(read.total_usd < out.total_usd);
    }

    #[test]
    fn unknown_model_contributes_zero_and_is_flagged() {
        let est = estimate_session_cost(&[mu("some-future-model-x", 1_000_000, 0, 0, 1_000_000)]);
        assert_eq!(est.total_usd, 0.0);
        assert_eq!(
            est.models_without_pricing,
            vec!["some-future-model-x".to_string()]
        );
        assert_eq!(est.per_model.len(), 1);
        assert!(!est.per_model[0].priced);
        assert_eq!(est.per_model[0].estimated_cost_usd, 0.0);
    }

    #[test]
    fn mixed_models_sum_only_priced() {
        let est = estimate_session_cost(&[
            mu("claude-opus-4-8", 0, 0, 0, 1_000_000),           // $25
            mu("claude-haiku-4-5-20251001", 0, 0, 0, 1_000_000), // $5
            mu("unknown-y", 0, 0, 0, 1_000_000),                 // $0, flagged
        ]);
        assert!((est.total_usd - 30.0).abs() < 1e-9, "got {}", est.total_usd);
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

    #[test]
    fn fable_5_is_priced_from_public_rates() {
        // platform.claude.com/docs/en/about-claude/pricing (captured 2026-06-11):
        // fable-5 input $10, 5m cache write $12.50, cache read $1, output $50.
        // 1M of each class → 10 + 12.5 + 1 + 50 = 73.5.
        let est = estimate_session_cost(&[mu(
            "claude-fable-5",
            1_000_000,
            1_000_000,
            1_000_000,
            1_000_000,
        )]);
        assert!((est.total_usd - 73.5).abs() < 1e-9, "got {}", est.total_usd);
        assert!(est.per_model[0].priced);
        assert!(est.models_without_pricing.is_empty());
    }

    #[test]
    fn sonnet_5_is_priced_from_public_rates() {
        // platform.claude.com/docs/en/about-claude/pricing: claude-sonnet-5
        // introductory pricing (through 2026-08-31): input $2, 5m cache write
        // $2.50, cache read $0.20, output $10. 1M of each → 2 + 2.5 + 0.2 + 10 = 14.7.
        // claude-sonnet-5 is a currently-active model and MUST be priced, else
        // live sonnet-5 sessions show as "미가격" (2026-07-05 사용자 관측).
        let est = estimate_session_cost(&[mu(
            "claude-sonnet-5",
            1_000_000,
            1_000_000,
            1_000_000,
            1_000_000,
        )]);
        assert!((est.total_usd - 14.7).abs() < 1e-9, "got {}", est.total_usd);
        assert!(est.per_model[0].priced);
        assert!(est.models_without_pricing.is_empty());
    }

    #[test]
    fn pricing_loads_from_checked_in_json() {
        // 가격표 SSOT는 저장소 루트 pricing.json (스펙 §2.1).
        assert!(rates_for("claude-fable-5").is_some());
        assert!(rates_for("claude-haiku-4-5-20251001").is_some());
        let v = pricing_version();
        let date = v
            .strip_prefix("pricing_estimate@")
            .expect("version must start with pricing_estimate@");
        assert!(
            chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").is_ok(),
            "version date must be YYYY-MM-DD, got {v}"
        );
        assert!(pricing_source_url().starts_with("https://"));
    }
}
