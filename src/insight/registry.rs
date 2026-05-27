//! Extractor registry — returns all registered `InsightExtractor` instances.
//!
//! The order here is canonical; `tests/insight_registry.rs` asserts it.
//! Adding or removing a category requires updating that test AND the category
//! table in `docs/superpowers/specs/2026-05-27-witmcc-insight-engine-architecture.md`.

use crate::insight::extractor::InsightExtractor;
use crate::insight::extractors::{missing_verification::MissingVerification, tool_failure::ToolFailure};

/// All currently registered L1 extractors, in stable order.
pub fn all_extractors() -> Vec<Box<dyn InsightExtractor>> {
    vec![
        Box::new(MissingVerification),  // slice-14
        Box::new(ToolFailure),          // slice-14
        // RiskyAction    — slice-16
        // ContextBloat   — slice-16
        // FinalStateMismatch — slice-16
    ]
}
