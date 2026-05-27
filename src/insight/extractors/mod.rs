//! Extractor implementations, one module per category.
//! Slice-14 ships two L1 deterministic extractors:
//!   - `missing_verification`: action episode with no following verification.
//!   - `tool_failure`: is_error=true with no compensating retry within 5 events.
//!
//! Slice-15: `noop_test` extractor added under `cfg(test)` — exercises the full
//! L2 code path (pending queue, cache, budget) without introducing a production
//! finding category.

pub mod missing_verification;
pub mod tool_failure;

/// Test-only extractor that always emits one candidate with PromotionPolicy::Never,
/// routing through the L2 judge path (DEV-S15-01). Only available when the
/// `test-helpers` feature is enabled — never compiled into production binaries.
#[cfg(feature = "test-helpers")]
pub mod noop_test {
    use crate::insight::extractor::InsightExtractor;
    use crate::insight::types::{FindingCandidate, PromotionPolicy};
    use crate::insight::view::SessionInsightView;

    pub struct NoopTestExtractor;

    impl InsightExtractor for NoopTestExtractor {
        fn category(&self) -> &'static str {
            "noop_test"
        }
        fn floor(&self) -> f32 {
            0.5
        }
        fn promotion_policy(&self) -> PromotionPolicy {
            PromotionPolicy::Never
        }
        fn extract(&self, _view: &SessionInsightView<'_>) -> Vec<FindingCandidate> {
            vec![FindingCandidate {
                category: "noop_test",
                confidence_l1: 0.9,
                severity: "low",
                summary: "noop_test: synthetic candidate for L2 path testing".to_string(),
                evidence_refs: vec!["ev_000".to_string()],
                evidence_projection: serde_json::json!({"synthetic": true}),
            }]
        }
    }
}
