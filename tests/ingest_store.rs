use sqlx::sqlite::SqlitePoolOptions;
use sqlx::Row as _Row;
use witmcc::db::{migrate, repo_observed};
use witmcc::ingest::store;

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
    let stats1 = store::ingest_file(&pool, path).await.unwrap();
    let stats2 = store::ingest_file(&pool, path).await.unwrap();
    assert!(stats1.observed_inserted > 0);
    assert_eq!(stats2.raw_inserted, 0, "second run inserts no new raw rows");
    let evs = repo_observed::list_session(&pool, "sess-A", 100)
        .await
        .unwrap();
    // Stable count regardless of how many runs were executed.
    assert_eq!(evs.len(), 6);
}

/// Regression for the slice-7 fix: `store::ingest_file` must populate
/// `graph_node` for the touched sessions. Without this, the WebUI's
/// SessionDetail timeline renders zero markers even though
/// `/v1/sessions/<id>` reports hundreds of events. Verified live on
/// bahamoth's 159-transcript-session DB: every transcript session had
/// `graph_node` = 0 because OTel ingest paths called
/// `graph::build::rebuild_session` but the transcript path did not.
#[tokio::test]
async fn ingest_file_populates_graph_nodes() {
    let pool = make_pool().await;
    let path = std::path::Path::new("tests/fixtures/transcripts/minimal_session.jsonl");
    store::ingest_file(&pool, path).await.unwrap();
    let node_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM graph_node WHERE session_id = 'sess-A'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        node_count.0 > 0,
        "graph_node must be populated after ingest_file; got {}",
        node_count.0
    );
}

/// Re-running ingest_file on the same fixture must not duplicate graph nodes —
/// the rebuild deletes + reinserts deterministically.
#[tokio::test]
async fn ingest_file_graph_rebuild_is_idempotent() {
    let pool = make_pool().await;
    let path = std::path::Path::new("tests/fixtures/transcripts/minimal_session.jsonl");
    store::ingest_file(&pool, path).await.unwrap();
    let first: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM graph_node WHERE session_id = 'sess-A'")
            .fetch_one(&pool)
            .await
            .unwrap();
    store::ingest_file(&pool, path).await.unwrap();
    let second: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM graph_node WHERE session_id = 'sess-A'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(first.0, second.0, "graph node count must be stable");
}
