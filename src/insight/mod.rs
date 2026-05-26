//! Slice-11 — M5 Insight engine entry point.
//!
//! The engine is a small registry of pure rules. Each rule consumes the
//! session's `ObservedEvent` array + the rebuilt graph nodes and emits zero
//! or more `NewFinding` rows. The async wrapper (`run_session`) fetches
//! events + nodes, calls the pure pipeline, and upserts results.
//!
//! Per CLAUDE.md "evidence-linked": every Finding *must* carry
//! `evidence_refs`. Rules that cannot supply evidence MUST NOT emit a row.
//!
//! Per CLAUDE.md "no annotation model": findings are rule-write-only. No
//! API surface mutates rows from outside.

pub mod rules;

use chrono::Utc;
use sqlx::SqlitePool;

use crate::db::{repo_finding, repo_graph, repo_observed};
use crate::error::Result;
use crate::model::graph::GraphNode;
use crate::model::observed::ObservedEvent;

pub use repo_finding::NewFinding;

/// Pure entry point — testable without DB. The async wrapper below feeds it.
pub fn run_session_pure(
    session_id: &str,
    events: &[ObservedEvent],
    graph_nodes: &[GraphNode],
) -> Vec<NewFinding> {
    let now = Utc::now().to_rfc3339();
    let mut out = Vec::new();
    for rule in rules::all() {
        out.extend(rule.evaluate(session_id, events, graph_nodes, &now));
    }
    out
}

/// Async wrapper — read events + graph from store, run rules, upsert
/// findings into `finding` table. Designed to be called after a graph
/// rebuild commits.
pub async fn run_session(pool: &SqlitePool, session_id: &str) -> Result<usize> {
    let events = repo_observed::list_session(pool, session_id, 100_000).await?;
    let (nodes, _edges) = repo_graph::load_session(pool, session_id).await?;
    let findings = run_session_pure(session_id, &events, &nodes);
    let n = findings.len();
    for f in findings {
        repo_finding::insert(pool, &f).await?;
    }
    Ok(n)
}
