//! Extractor registry — returns all registered `InsightExtractor` instances.
//!
//! The order here is canonical; `tests/insight_registry.rs` asserts it.
//! Adding or removing a category requires updating that test AND the category
//! table in `docs/superpowers/specs/2026-05-27-witmcc-insight-engine-architecture.md`.

use crate::insight::extractor::InsightExtractor;
use crate::insight::extractors::{
    context_bloat::ContextBloat,
    final_state_mismatch::FinalStateMismatch,
    missing_verification::MissingVerification,
    risky_action::RiskyAction,
    tool_failure::ToolFailure,
};

/// All currently registered extractors (5 MVP categories), in stable order.
/// Slice-14: MissingVerification, ToolFailure (L1/Always).
/// Slice-16: RiskyAction, ContextBloat, FinalStateMismatch (L1+L2/IfAbove(1.0)).
pub fn all_extractors() -> Vec<Box<dyn InsightExtractor>> {
    vec![
        Box::new(MissingVerification),   // slice-14 — L1/Always
        Box::new(ToolFailure),           // slice-14 — L1/Always
        Box::new(RiskyAction),           // slice-16 — L1+L2/IfAbove(1.0)
        Box::new(ContextBloat),          // slice-16 — L1+L2/IfAbove(1.0)
        Box::new(FinalStateMismatch),    // slice-16 — L1+L2/IfAbove(1.0)
    ]
}
