//! Rule registry. Slice-11 ships one rule (`tool_failure`); follow-up
//! slices add the remaining four categories from spec §08.

pub mod tool_failure;

use crate::db::repo_finding::NewFinding;
use crate::model::graph::GraphNode;
use crate::model::observed::ObservedEvent;

pub trait Rule: Sync + Send {
    fn name(&self) -> &'static str;
    fn evaluate(
        &self,
        session_id: &str,
        events: &[ObservedEvent],
        graph_nodes: &[GraphNode],
        generated_at: &str,
    ) -> Vec<NewFinding>;
}

/// Static rule list. Order is stable so `run_session_pure` output ordering
/// is deterministic across runs.
pub fn all() -> Vec<Box<dyn Rule>> {
    vec![Box::new(tool_failure::ToolFailureRule)]
}
