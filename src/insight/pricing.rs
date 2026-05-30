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
