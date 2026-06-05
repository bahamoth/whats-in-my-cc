//! Bootstrap script to compute inferred edge counts for the golden fixture.
//!
//! Run with:
//!   cargo run --example bootstrap_inferred_edge_counts
//!
//! Writes `tests/fixtures/inferred_edge_counts.json` with counts from each
//! real fixture JSONL. Committed result becomes the regression golden.
//!
//! This example is for reproducibility — do not call from production code.

use sqlx::sqlite::SqlitePoolOptions;
use std::collections::BTreeMap;
use wimcc::db::{migrate, repo_diff_hunk, repo_observed, repo_verification_run};
use wimcc::graph::build::compute;
use wimcc::ingest::store;
use wimcc::insight::edge_inference::{all_rules, SessionGraphView};
use wimcc::live::NoopSink;

/// Map fixture file name to session_id prefix (first 8 chars).
const FIXTURES: &[(&str, &str)] = &[
    ("verification_v01", "aac68973"),
    ("structured_patch_v01", "ed82aee9"),
];

#[tokio::main]
async fn main() {
    let mut by_session: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();

    for (fixture_name, session_prefix) in FIXTURES {
        println!("Loading fixture: {fixture_name}...");
        let pool = SqlitePoolOptions::new()
            .max_connections(2)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        migrate(&pool).await.unwrap();
        let path = format!("tests/fixtures/transcripts/real/{fixture_name}.jsonl");
        store::ingest_file(&pool, std::path::Path::new(&path), &NoopSink)
            .await
            .unwrap_or_else(|e| panic!("ingest failed for {fixture_name}: {e}"));
        let sessions = repo_observed::list_sessions(&pool, 10).await.unwrap();
        if sessions.is_empty() {
            eprintln!("WARNING: no sessions found in {fixture_name}; skipping");
            continue;
        }
        let session_id = sessions[0].session_id.clone();
        assert!(
            session_id.starts_with(session_prefix),
            "session_id {session_id} does not match prefix {session_prefix}"
        );
        let evs = repo_observed::list_session(&pool, &session_id, 100_000)
            .await
            .unwrap();
        let hunks = repo_diff_hunk::list_session(&pool, &session_id)
            .await
            .unwrap_or_default();
        let runs = repo_verification_run::list_session(&pool, &session_id)
            .await
            .unwrap_or_default();
        let (nodes, det_edges) = compute(&session_id, &evs, &hunks, &runs);
        let view = SessionGraphView {
            session_id: &session_id,
            events: &evs,
            nodes: &nodes,
            deterministic_edges: &det_edges,
        };

        let mut rule_counts: BTreeMap<String, usize> = BTreeMap::new();
        for rule in all_rules() {
            let edges = rule.infer(&view);
            println!("  {} -> {} rule={}", fixture_name, edges.len(), rule.rule_id());
            rule_counts.insert(rule.rule_id().to_string(), edges.len());
        }
        by_session.insert(session_id.clone(), rule_counts);
    }

    let output = serde_json::json!({
        "schema_version": "inferred_edge_counts.v1",
        "by_session_and_rule": by_session,
    });
    let json = serde_json::to_string_pretty(&output).unwrap();
    std::fs::write("tests/fixtures/inferred_edge_counts.json", &json).unwrap();
    println!("\nWrote tests/fixtures/inferred_edge_counts.json");
    println!("{json}");
}
