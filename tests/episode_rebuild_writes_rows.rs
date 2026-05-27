//! Slice-12 — asserts that rebuild_session writes episode rows for a real session.
//!
//! This test is red until rebuild_session calls the episode classifier.

use sqlx::sqlite::SqlitePoolOptions;
use witmcc::db::{migrate, repo_episode};
use witmcc::graph::build;
use witmcc::ingest::store;
use witmcc::live::NoopSink;

#[tokio::test]
async fn rebuild_session_writes_episode_rows() {
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    store::ingest_file(
        &pool,
        std::path::Path::new("tests/fixtures/transcripts/real/verification_v01.jsonl"),
        &NoopSink,
    )
    .await
    .unwrap();

    let sessions = witmcc::db::repo_observed::list_sessions(&pool, 10)
        .await
        .unwrap();
    assert!(!sessions.is_empty());
    let session_id = sessions[0].session_id.clone();

    build::rebuild_session(&pool, &session_id).await.unwrap();

    let episodes = repo_episode::list_session(&pool, &session_id).await.unwrap();
    assert!(
        !episodes.is_empty(),
        "rebuild_session must write at least one episode row; got 0"
    );
    // Every episode must have a valid phase.
    let valid_phases = ["intake", "exploration", "diagnosis", "action", "verification", "repair", "drift"];
    for ep in &episodes {
        assert!(
            valid_phases.contains(&ep.phase.as_str()),
            "unexpected phase: {}",
            ep.phase
        );
    }
}
