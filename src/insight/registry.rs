//! Extractor registry — returns all registered `InsightExtractor` instances.
//!
//! The order here is canonical; `tests/insight_registry.rs` asserts it.
//! Adding or removing a category requires updating that test AND the category
//! table in `docs/superpowers/specs/2026-05-27-wimcc-insight-engine-architecture.md`.

use crate::insight::extractor::InsightExtractor;
use crate::insight::extractors::{
    context_bloat::ContextBloat,
    final_state_mismatch::FinalStateMismatch,
    risky_action::RiskyAction,
    tool_failure::ToolFailure,
};

/// All currently registered extractors (4 MVP categories), in stable order.
/// All deterministic L1: every candidate >= its floor promotes directly.
/// Slice-14: ToolFailure. Slice-16: RiskyAction, ContextBloat, FinalStateMismatch.
pub fn all_extractors() -> Vec<Box<dyn InsightExtractor>> {
    vec![
        Box::new(ToolFailure),           // slice-14 — L1
        Box::new(RiskyAction),           // slice-16 — L1
        Box::new(ContextBloat),          // slice-16 — L1
        Box::new(FinalStateMismatch),    // slice-16 — L1
    ]
}
