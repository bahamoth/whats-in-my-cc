//! Slice-12 — determinism invariant for the episode classifier.
//!
//! Running the classifier twice on the same input must return identical
//! episode_ids (which are sha256 of session_id||phase||start||end).

use sqlx::sqlite::SqlitePoolOptions;
use witmcc::db::{migrate, repo_observed};
use witmcc::ingest::store;
use witmcc::insight::episode::classifier::classify_session;
use witmcc::live::NoopSink;

#[tokio::test]
async fn classifier_is_deterministic() {
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
    let sessions = repo_observed::list_sessions(&pool, 10).await.unwrap();
    assert!(!sessions.is_empty());
    let session_id = &sessions[0].session_id;

    let evs = repo_observed::list_session(&pool, session_id, 100_000)
        .await
        .unwrap();

    let run1 = classify_session(session_id, &evs, &[]);
    let run2 = classify_session(session_id, &evs, &[]);

    assert_eq!(
        run1.len(),
        run2.len(),
        "classifier must produce the same episode count on two runs"
    );
    for (a, b) in run1.iter().zip(run2.iter()) {
        assert_eq!(a.episode_id, b.episode_id, "episode_ids must be identical");
        assert_eq!(a.phase, b.phase, "phases must be identical");
        assert_eq!(
            a.start_event_id, b.start_event_id,
            "start_event_id must be identical"
        );
        assert_eq!(
            a.end_event_id, b.end_event_id,
            "end_event_id must be identical"
        );
    }
}
