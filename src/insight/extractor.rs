//! `InsightExtractor` trait — the contract every L1 category must implement.
//! Pure CPU work; no DB calls, no I/O, no side effects inside `extract()`.

use crate::insight::types::{FindingCandidate, PromotionPolicy};
use crate::insight::view::SessionInsightView;

/// Implemented by each L1 extractor category.
pub trait InsightExtractor: Send + Sync {
    /// Stable category string — appears in `Finding.category`.
    fn category(&self) -> &'static str;

    /// L1 confidence floor — finding confidence never stored below this.
    fn floor(&self) -> f32;

    /// Promotion policy for this category.
    fn promotion_policy(&self) -> PromotionPolicy;

    /// Pure CPU extraction against the loaded session view.
    /// Same input always produces the same output (idempotent).
    fn extract(&self, view: &SessionInsightView<'_>) -> Vec<FindingCandidate>;
}
