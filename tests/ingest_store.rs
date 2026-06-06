use sqlx::sqlite::SqlitePoolOptions;
use wimcc::db::{migrate, repo_observed};
use wimcc::ingest::store;

async fn make_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    pool
}

#[tokio::test]
async fn ingest_minimal_fixture_twice_is_idempotent() {
    let pool = make_pool().await;
    let path = std::path::Path::new("tests/fixtures/transcripts/minimal_session.jsonl");
    let stats1 = store::ingest_file(&pool, path, &wimcc::live::NoopSink).await.unwrap();
    let stats2 = store::ingest_file(&pool, path, &wimcc::live::NoopSink).await.unwrap();
    assert!(stats1.observed_inserted > 0);
    assert_eq!(stats2.raw_inserted, 0, "second run inserts no new raw rows");
    let evs = repo_observed::list_session(&pool, "sess-A", 100)
        .await
        .unwrap();
    // Stable count regardless of how many runs were executed.
    assert_eq!(evs.len(), 6);
}

/// `store::ingest_file` must persist observed_event rows for the touched
/// session (the SSOT the WebUI timeline and all views read from).
#[tokio::test]
async fn ingest_file_populates_observed_events() {
    let pool = make_pool().await;
    let path = std::path::Path::new("tests/fixtures/transcripts/minimal_session.jsonl");
    store::ingest_file(&pool, path, &wimcc::live::NoopSink).await.unwrap();
    let evs = repo_observed::list_session(&pool, "sess-A", 100).await.unwrap();
    assert!(
        !evs.is_empty(),
        "observed_event must be populated after ingest_file; got 0"
    );
}

/// Re-running ingest_file on the same fixture must not duplicate observed rows —
/// ingest is idempotent.
#[tokio::test]
async fn ingest_file_observed_events_are_idempotent() {
    let pool = make_pool().await;
    let path = std::path::Path::new("tests/fixtures/transcripts/minimal_session.jsonl");
    store::ingest_file(&pool, path, &wimcc::live::NoopSink).await.unwrap();
    let first = repo_observed::list_session(&pool, "sess-A", 100).await.unwrap().len();
    store::ingest_file(&pool, path, &wimcc::live::NoopSink).await.unwrap();
    let second = repo_observed::list_session(&pool, "sess-A", 100).await.unwrap().len();
    assert_eq!(first, second, "observed_event count must be stable");
}
