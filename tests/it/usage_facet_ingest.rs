//! Real-data anchoring: ingesting the frozen verification_v01 transcript must
//! populate usage_facet with real token counts (cache_read > 0).
use sqlx::sqlite::SqlitePoolOptions;
use wimcc::db::{migrate, repo_usage_facet};
use wimcc::ingest::store;
use wimcc::live::NoopSink;

async fn empty_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    pool
}

#[tokio::test]
async fn ingest_populates_usage_facet_from_real_fixture() {
    let pool = empty_pool().await;
    store::ingest_file(
        &pool,
        std::path::Path::new("tests/fixtures/transcripts/real/verification_v01.jsonl"),
        &NoopSink,
    )
    .await
    .expect("ingest");

    let agg = repo_usage_facet::session_aggregate(&pool, "aac68973-729e-4014-a02b-28a556f5ff29")
        .await
        .expect("aggregate");

    assert!(
        agg.assistant_events > 0,
        "expected assistant events with usage"
    );
    assert!(
        agg.cache_read_input_tokens > 0,
        "real fixture has prompt-cache reads"
    );
}
