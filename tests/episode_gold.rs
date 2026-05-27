//! Slice-12 — golden regression tests for the episode classifier.
//!
//! Each golden file in `tests/fixtures/episode_gold/*.json` corresponds to a
//! real transcript fixture. Tests verify that the classifier produces the same
//! phase sequence and episode count as the recorded golden.
//!
//! Updating a golden requires:
//! 1. Justification in the commit body (why did a boundary move?).
//! 2. A new implementation-notes entry if the rule version changed.
//!
//! Golden files bootstrapped from:
//! - `verification_v01.jsonl` → session prefix `aac68973` (6 events, 3 runs)
//! - `structured_patch_v01.jsonl` → session prefix `ed82aee9` (2 events, 0 runs)
//!
//! Real-data anchors per CLAUDE.md §Working Principles:
//! - `aac68973`: 6 events = 3 Bash tool_calls + 3 tool_results (is_error=false).
//!   Each Bash call is a cargo verification command → 3 VerificationRunRows (slice-11).
//!   Expected: 3 action + 3 verification episodes alternating (action→verification×3).
//! - `ed82aee9`: 2 events = 1 assistant create-file tool_result pair. No verification runs.
//!   Expected: 1 intake episode (fixture starts with a tool_result; no user_message prior).

use serde::Deserialize;
use sqlx::sqlite::SqlitePoolOptions;
use witmcc::db::{migrate, repo_observed, repo_verification_run};
use witmcc::ingest::store;
use witmcc::insight::episode::classifier::classify_session;
use witmcc::live::NoopSink;

#[derive(Deserialize)]
struct Gold {
    expected_episodes: Vec<ExpectedEpisode>,
}

#[derive(Deserialize)]
struct ExpectedEpisode {
    phase: String,
    #[allow(dead_code)]
    start_event_offset_in_session: usize,
    #[allow(dead_code)]
    end_event_offset_in_session: usize,
    #[allow(dead_code)]
    classification_basis: Vec<String>,
}

/// Load both events and verification runs from a transcript fixture in a single
/// in-memory DB so trigger_event_id references inside VerificationRunRow match
/// the event_ids in the ObservedEvent slice.
async fn load_fixture(fixture_name: &str) -> (
    String,
    Vec<witmcc::model::observed::ObservedEvent>,
    Vec<witmcc::db::repo_verification_run::VerificationRunRow>,
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
    assert!(!sessions.is_empty(), "fixture {fixture_name} produced no sessions");
    let session_id = sessions[0].session_id.clone();
    let evs = repo_observed::list_session(&pool, &session_id, 100_000)
        .await
        .unwrap();
    let runs = repo_verification_run::list_session(&pool, &session_id)
        .await
        .unwrap_or_default();
    (session_id, evs, runs)
}

fn run_golden_check(
    fixture_name: &str,
    golden_key: &str,
    session_id: &str,
    evs: &[witmcc::model::observed::ObservedEvent],
    runs: &[witmcc::db::repo_verification_run::VerificationRunRow],
) {
    let got = classify_session(session_id, evs, runs);
    let gold_path = format!("tests/fixtures/episode_gold/{golden_key}.json");
    let gold_json = std::fs::read_to_string(&gold_path)
        .unwrap_or_else(|e| panic!("cannot read golden {gold_path}: {e}"));
    let want: Gold = serde_json::from_str(&gold_json)
        .unwrap_or_else(|e| panic!("cannot parse golden {gold_path}: {e}"));

    assert_eq!(
        got.len(),
        want.expected_episodes.len(),
        "fixture={fixture_name}: episode count diverged: got {}, expected {}",
        got.len(),
        want.expected_episodes.len()
    );

    for (i, (g, w)) in got.iter().zip(want.expected_episodes.iter()).enumerate() {
        assert_eq!(
            format!("{:?}", g.phase).to_lowercase(),
            w.phase,
            "fixture={fixture_name} episode {i} phase"
        );
    }
}

#[tokio::test]
async fn aac68973_golden_matches() {
    // verification_v01.jsonl = session aac68973 (6 events, 3 runs, 6 expected episodes)
    let (session_id, evs, runs) = load_fixture("verification_v01").await;
    run_golden_check("verification_v01", "aac68973", &session_id, &evs, &runs);
}

#[tokio::test]
async fn ed82aee9_golden_matches() {
    // structured_patch_v01.jsonl = session ed82aee9 (2 events, 0 runs, 1 expected episode)
    let (session_id, evs, runs) = load_fixture("structured_patch_v01").await;
    run_golden_check("structured_patch_v01", "ed82aee9", &session_id, &evs, &runs);
}
