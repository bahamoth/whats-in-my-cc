//! `caused_repair@v1` — error tool_call → next overlapping repair tool_call.
//!
//! Frozen thresholds (DEV-S13-02):
//!   N = 60 s  (max gap between error and repair)
//!   K = 2     (min shared tokens after stop-word removal)

use crate::model::graph::GraphEdge;
use crate::insight::edge_inference::{EdgeInferenceRule, SessionGraphView};

/// Rule ID for this version; frozen — do not change.
pub const RULE_ID: &str = "caused_repair@v1";

pub struct CausedRepairV1;

impl EdgeInferenceRule for CausedRepairV1 {
    fn rule_id(&self) -> &'static str {
        RULE_ID
    }

    fn infer(&self, _view: &SessionGraphView<'_>) -> Vec<GraphEdge> {
        // Stub — returns empty until Phase 3
        vec![]
    }
}
