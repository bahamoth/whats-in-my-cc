//! `SessionInsightView` — the single read-only context object handed to every
//! extractor in a `rebuild_session` cycle. Constructed once, used by all.

use sqlx::SqlitePool;

use crate::db::{repo_diff_hunk, repo_graph, repo_observed, repo_verification_run};
use crate::db::repo_diff_hunk::DiffHunkRow;
use crate::db::repo_verification_run::VerificationRunRow;
use crate::error::Result;
use crate::model::graph::{GraphEdge, GraphNode};
use crate::model::observed::ObservedEvent;

/// All session data needed by every extractor, loaded once before the pipeline runs.
pub struct SessionInsightView<'a> {
    pub session_id: &'a str,
    pub events: &'a [ObservedEvent],
    pub diff_hunks: &'a [DiffHunkRow],
    pub verification_runs: &'a [VerificationRunRow],
    pub nodes: &'a [GraphNode],
    pub edges: &'a [GraphEdge],
}

/// Owned version of the view, loaded from the DB.
/// The `'a`-lifetime struct borrows from this.
pub struct OwnedSessionInsightData {
    pub events: Vec<ObservedEvent>,
    pub diff_hunks: Vec<DiffHunkRow>,
    pub verification_runs: Vec<VerificationRunRow>,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

impl OwnedSessionInsightData {
    /// Load all session data from the DB in one batch of reads.
    pub async fn load(pool: &SqlitePool, session_id: &str) -> Result<Self> {
        let events = repo_observed::list_session(pool, session_id, 100_000).await?;
        let diff_hunks = repo_diff_hunk::list_session(pool, session_id).await?;
        let verification_runs = repo_verification_run::list_session(pool, session_id).await?;
        let (nodes, edges) = repo_graph::load_session(pool, session_id).await?;
        Ok(Self {
            events,
            diff_hunks,
            verification_runs,
            nodes,
            edges,
        })
    }

    /// Borrow as a `SessionInsightView` for passing to extractors.
    pub fn as_view<'a>(&'a self, session_id: &'a str) -> SessionInsightView<'a> {
        SessionInsightView {
            session_id,
            events: &self.events,
            diff_hunks: &self.diff_hunks,
            verification_runs: &self.verification_runs,
            nodes: &self.nodes,
            edges: &self.edges,
        }
    }
}
