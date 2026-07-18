//! Recompute policy split — live tail vs CLI replay (2026-07-18).
//!
//! Dogfooding measurement 2026-07-18: a live serve pegged one core because
//! every debounced flush re-ran the full insight pipeline for the session even
//! when every line deduped (0 new rows). The fix separates two intents:
//! - `ingest_file_live` (transcript tail): recompute only sessions that gained
//!   at least one new raw row this run.
//! - `ingest_file` / `ingest_paths` (CLI `wimcc ingest`): recompute every
//!   touched session even on full dedup — replaying transcripts after a parser
//!   upgrade must refresh derived rows (the documented reason skip-lines still
//!   mark sessions as touched).

use sqlx::sqlite::SqlitePoolOptions;
use wimcc::ingest::store;
use wimcc::live::NoopSink;

async fn make_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    wimcc::db::migrate(&pool).await.unwrap();
    pool
}

const FIXTURE: &str = "tests/fixtures/transcripts/minimal_session.jsonl";

#[tokio::test]
async fn live_reingest_with_no_new_rows_skips_recompute() {
    let pool = make_pool().await;
    let path = std::path::Path::new(FIXTURE);

    let stats1 = store::ingest_file_live(&pool, path, &NoopSink).await.unwrap();
    assert!(stats1.raw_inserted > 0);
    assert!(
        stats1.sessions_recomputed.contains("sess-A"),
        "first live ingest inserts rows and recomputes; got {:?}",
        stats1.sessions_recomputed
    );

    let stats2 = store::ingest_file_live(&pool, path, &NoopSink).await.unwrap();
    assert_eq!(stats2.raw_inserted, 0, "second run fully dedups");
    assert!(
        stats2.sessions_touched.contains("sess-A"),
        "dedup still marks the session as touched"
    );
    assert!(
        stats2.sessions_recomputed.is_empty(),
        "0 new rows on the live path must skip recompute; got {:?}",
        stats2.sessions_recomputed
    );
}

/// Every `ingest_paths` call starts an `ingest_run` row. On the live path a
/// fully-deduped flush (one per 100ms debounce window while a session is
/// active) must not leave that row behind — the dogfood DB accumulated 9k+
/// empty run rows in 5 weeks (2026-07-18 measurement). Runs that captured raw
/// rows keep their row (FK from raw_event, provenance).
#[tokio::test]
async fn live_flush_with_no_new_rows_leaves_no_ingest_run_row() {
    let pool = make_pool().await;
    let path = std::path::Path::new(FIXTURE);

    store::ingest_file_live(&pool, path, &NoopSink).await.unwrap();
    store::ingest_file_live(&pool, path, &NoopSink).await.unwrap();

    let runs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ingest_run")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        runs, 1,
        "the second (fully-deduped) live flush must delete its empty ingest_run row"
    );
}

#[tokio::test]
async fn cli_replay_with_no_new_rows_still_recomputes() {
    let pool = make_pool().await;
    let path = std::path::Path::new(FIXTURE);

    store::ingest_file(&pool, path, &NoopSink).await.unwrap();
    let stats2 = store::ingest_file(&pool, path, &NoopSink).await.unwrap();
    assert_eq!(stats2.raw_inserted, 0, "replay fully dedups");
    assert!(
        stats2.sessions_recomputed.contains("sess-A"),
        "CLI replay must recompute touched sessions even on full dedup \
         (parser-upgrade refresh semantics); got {:?}",
        stats2.sessions_recomputed
    );
}
