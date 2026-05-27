//! `triggered_by_user_message@v1` — user_message → next tool_call with no
//! preceding assistant text in the same turn.
//!
//! Frozen threshold (DEV-S13-02): confidence = 0.85 fixed.

use crate::model::graph::GraphEdge;
use crate::insight::edge_inference::{EdgeInferenceRule, SessionGraphView};

/// Rule ID for this version; frozen — do not change.
pub const RULE_ID: &str = "triggered_by_user_message@v1";

pub struct TriggeredByUserMessageV1;

impl EdgeInferenceRule for TriggeredByUserMessageV1 {
    fn rule_id(&self) -> &'static str {
        RULE_ID
    }

    fn infer(&self, _view: &SessionGraphView<'_>) -> Vec<GraphEdge> {
        // Stub — returns empty until Phase 3
        vec![]
    }
}
