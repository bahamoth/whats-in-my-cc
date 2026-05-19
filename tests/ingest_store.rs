use sqlx::sqlite::SqlitePoolOptions;
use witmcc::db::{migrate, repo_observed};
use witmcc::ingest::store;

#[tokio::test]
async fn ingest_minimal_fixture_twice_is_idempotent() {
    let pool = SqlitePoolOptions::new().max_connections(2).connect("sqlite::memory:").await.unwrap();
    migrate(&pool).await.unwrap();
    let path = std::path::Path::new("tests/fixtures/transcripts/minimal_session.jsonl");
    let stats1 = store::ingest_file(&pool, path).await.unwrap();
    let stats2 = store::ingest_file(&pool, path).await.unwrap();
    assert!(stats1.observed_inserted > 0);
    assert_eq!(stats2.raw_inserted, 0, "second run inserts no new raw rows");
    let evs = repo_observed::list_session(&pool, "sess-A", 100).await.unwrap();
    // Stable count regardless of how many runs were executed.
    assert_eq!(evs.len(), 6);
}
