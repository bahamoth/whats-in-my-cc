//! `large_output_to_next_action@v1` — oversized tool_result → next assistant_message.
//!
//! Frozen thresholds (DEV-S13-02):
//!   T = 50 * 1024 bytes (51_200)
//!   confidence = 0.6 + 0.4 * normalise(size / max_session_size), clamped [0.6, 1.0]

use crate::model::graph::GraphEdge;
use crate::insight::edge_inference::{EdgeInferenceRule, SessionGraphView};

/// Rule ID for this version; frozen — do not change.
pub const RULE_ID: &str = "large_output_to_next_action@v1";

pub struct LargeOutputToNextActionV1;

impl EdgeInferenceRule for LargeOutputToNextActionV1 {
    fn rule_id(&self) -> &'static str {
        RULE_ID
    }

    fn infer(&self, _view: &SessionGraphView<'_>) -> Vec<GraphEdge> {
        // Stub — returns empty until Phase 3
        vec![]
    }
}
