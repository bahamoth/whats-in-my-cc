//! Slice-12 — golden regression test for the episode classifier.
//!
//! The golden file `tests/fixtures/episode_gold/aac68973.json` starts empty
//! (`expected_episodes: []`). Once the classifier implementation is green
//! (commit 3), commit 4 populates the golden by running the classifier against
//! the real fixtures and capturing output. After that point, any change to the
//! classifier that alters episode boundaries produces a diff in the golden —
//! which becomes the unit of review.
//!
//! Real-data anchor: `tests/fixtures/transcripts/real/verification_v01.jsonl`
//! is a frozen excerpt from session `aac68973` (14 events, 3 cargo-test pairs).
//! The session_id in that fixture is `aac68973-729e-4014-a02b-28a556f5ff29`;
//! for golden purposes we key on the short prefix `aac68973`.

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

/// Load ObservedEvents from a transcript fixture file via in-memory DB.
async fn load_real_transcript(fixture_name: &str) -> Vec<witmcc::model::observed::ObservedEvent> {
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
    assert!(!sessions.is_empty(), "fixture produced no sessions");
    repo_observed::list_session(&pool, &sessions[0].session_id, 100_000)
        .await
        .unwrap()
}

/// Load VerificationRunRows for the first session in a fixture.
/// In the current test phase the verification_run table is populated by
/// ingest; however, classification tests also work with an empty run list
/// (the golden will record whatever the real data yields).
async fn load_real_verification_runs_for(
    fixture_name: &str,
) -> Vec<witmcc::db::repo_verification_run::VerificationRunRow> {
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
    if sessions.is_empty() {
        return vec![];
    }
    repo_verification_run::list_session(&pool, &sessions[0].session_id)
        .await
        .unwrap_or_default()
}

#[tokio::test]
async fn aac68973_golden_matches() {
    // Uses the verification_v01 fixture (session aac68973).
    let evs = load_real_transcript("verification_v01").await;
    let runs = load_real_verification_runs_for("verification_v01").await;
    let got = classify_session("aac68973", &evs, &runs);

    let gold_json =
        std::fs::read_to_string("tests/fixtures/episode_gold/aac68973.json").unwrap();
    let want: Gold = serde_json::from_str(&gold_json).unwrap();

    assert_eq!(
        got.len(),
        want.expected_episodes.len(),
        "episode count diverged: got {}, expected {}",
        got.len(),
        want.expected_episodes.len()
    );

    for (i, (g, w)) in got.iter().zip(want.expected_episodes.iter()).enumerate() {
        assert_eq!(
            format!("{:?}", g.phase).to_lowercase(),
            w.phase,
            "episode {i} phase"
        );
    }
}
