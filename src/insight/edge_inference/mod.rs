//! Edge inference engine — slice-13.
//!
//! Each rule is a versioned pure function implementing `EdgeInferenceRule`.
//! Thresholds are frozen constants inside each `_v1.rs` file; bumping them
//! requires a new `_v2.rs` (never in-place edits to v1).

pub mod rules;

use crate::model::graph::{GraphEdge, GraphNode};
use crate::model::observed::ObservedEvent;

/// Version-stable canonical rule IDs, one per rule file.
pub const RULE_IDS: &[&str] = &[];

/// Shared read-only view of the complete session passed to each rule.
pub struct SessionGraphView<'a> {
    pub session_id: &'a str,
    pub events: &'a [ObservedEvent],
    pub nodes: &'a [GraphNode],
    pub deterministic_edges: &'a [GraphEdge],
}

/// Every inference rule must implement this trait.
pub trait EdgeInferenceRule: Send + Sync {
    /// Returns the versioned canonical rule ID (e.g. `"caused_repair@v1"`).
    fn rule_id(&self) -> &'static str;
    /// Produce inferred edges for the given session view.
    /// Must be pure — same input always produces the same output.
    fn infer(&self, view: &SessionGraphView<'_>) -> Vec<GraphEdge>;
}

/// Instantiate all registered v1 rules. Used by `compute()` and the counts test.
pub fn all_rules() -> Vec<Box<dyn EdgeInferenceRule>> {
    vec![]
}
