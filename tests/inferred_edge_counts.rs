//! Frozen golden counts for inferred edges per (session, rule).
//!
//! Bootstrap: empty golden passes trivially (commit 1).
//! Armed: populated golden from real transcript fixture runs (commit 4).
//!
//! Uses the same fixture-load pattern as `episode_gold.rs`:
//!   `store::ingest_file` on `tests/fixtures/transcripts/real/*.jsonl`.

use sqlx::sqlite::SqlitePoolOptions;
use wimcc::db::{migrate, repo_diff_hunk, repo_observed, repo_verification_run};
use wimcc::graph::build::compute;
use wimcc::ingest::store;
use wimcc::insight::edge_inference::{all_rules, SessionGraphView};
use wimcc::live::NoopSink;

async fn load_fixture(
    fixture_name: &str,
) -> (
    String,
    Vec<wimcc::model::observed::ObservedEvent>,
    Vec<wimcc::db::repo_diff_hunk::DiffHunkRow>,
    Vec<wimcc::db::repo_verification_run::VerificationRunRow>,
) {
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    let path = format!("tests/fixtures/transcripts/real/{fixture_name}.jsonl");
    store::ingest_file(&pool, std::path::Path::new(&path), &NoopSink)
        .await
        .unwrap();
    let sessions = repo_observed::list_sessions(&pool, 10).await.unwrap();
    assert!(
        !sessions.is_empty(),
        "fixture {fixture_name} produced no sessions"
    );
    let session_id = sessions[0].session_id.clone();
    let evs = repo_observed::list_session(&pool, &session_id, 100_000)
        .await
        .unwrap();
    let hunks = repo_diff_hunk::list_session(&pool, &session_id)
        .await
        .unwrap_or_default();
    let runs = repo_verification_run::list_session(&pool, &session_id)
        .await
        .unwrap_or_default();
    (session_id, evs, hunks, runs)
}

fn run_rule_by_id(
    rule_id: &str,
    view: &SessionGraphView<'_>,
) -> Vec<wimcc::model::graph::GraphEdge> {
    for rule in all_rules() {
        if rule.rule_id() == rule_id {
            return rule.infer(view);
        }
    }
    panic!("unknown rule_id: {rule_id}");
}

/// Maps session_id (full UUID or prefix) to fixture file name (without .jsonl).
fn fixture_for_session(session_id: &str) -> Option<&'static str> {
    if session_id.starts_with("aac68973") {
        Some("verification_v01")
    } else if session_id.starts_with("ed82aee9") {
        Some("structured_patch_v01")
    } else {
        None
    }
}

#[tokio::test]
async fn counts_match_golden_for_each_session() {
    let raw = std::fs::read_to_string("tests/fixtures/inferred_edge_counts.json")
        .expect("inferred_edge_counts.json must exist");
    let want: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let by_session = want["by_session_and_rule"]
        .as_object()
        .expect("by_session_and_rule must be an object");

    for (sid, rules) in by_session {
        let fixture = match fixture_for_session(sid) {
            Some(f) => f,
            None => {
                eprintln!("SKIP: no fixture mapping for session {sid}");
                continue;
            }
        };
        let (session_id, evs, hunks, runs) = load_fixture(fixture).await;
        let (nodes, det_edges) = compute(&session_id, &evs, &hunks, &runs);
        let view = SessionGraphView {
            session_id: &session_id,
            events: &evs,
            nodes: &nodes,
            deterministic_edges: &det_edges,
        };
        for (rule_id, want_count) in rules.as_object().unwrap() {
            let got_count = run_rule_by_id(rule_id, &view).len();
            let want_n = want_count.as_u64().unwrap() as usize;
            assert_eq!(
                got_count, want_n,
                "session {session_id} rule {rule_id}: got {got_count} want {want_n}"
            );
        }
    }
}
