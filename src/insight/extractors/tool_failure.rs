//! `ToolFailure` L1 extractor stub (slice-14).
//! Full implementation lands in Phase 4.

use crate::insight::extractor::InsightExtractor;
use crate::insight::types::{FindingCandidate, PromotionPolicy};
use crate::insight::view::SessionInsightView;

pub struct ToolFailure;

impl InsightExtractor for ToolFailure {
    fn category(&self) -> &'static str {
        "tool_failure"
    }

    fn floor(&self) -> f32 {
        1.0
    }

    fn promotion_policy(&self) -> PromotionPolicy {
        PromotionPolicy::Always
    }

    fn extract(&self, _view: &SessionInsightView<'_>) -> Vec<FindingCandidate> {
        vec![] // stub — Phase 4 implements the rule
    }
}
