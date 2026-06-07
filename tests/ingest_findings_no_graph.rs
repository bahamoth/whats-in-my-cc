//! Regression guard: ingesting a transcript must still produce signals WITHOUT
//! the graph layer.
//!
//! Before the graph refactor, `store::ingest_file` ran the deterministic insight
//! pipeline as a side effect of the graph rebuild. After the graph layer was
//! deleted, ingest calls `insight::pipeline::run_detectors` directly. This test
//! pins that behaviour: ingest a real-shaped transcript that contains one Bash
//! `cargo build` failure and assert that `repo_signal::list_by_session` returns
//! a non-empty set including a `tool_failure` signal.
//!
//! Fixture payload shape (tool_use / tool_result with `is_error`) is the frozen
//! real transcript shape verified against real fixture
//! aac68973-729e-4014-a02b-28a556f5ff29 (see `tests/extractor_tool_failure.rs`).

use sqlx::sqlite::SqlitePoolOptions;
use wimcc::db::{migrate, repo_signal};
use wimcc::ingest::store;
use wimcc::live::NoopSink;

const SESSION: &str = "tf0001-aaaa-bbbb-cccc-000000000001";
const FIXTURE: &str = "tests/fixtures/transcripts/real/tool_failure_v01.jsonl";

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
async fn ingest_produces_signals_without_graph() {
    let pool = make_pool().await;
    store::ingest_file(&pool, std::path::Path::new(FIXTURE), &NoopSink)
        .await
        .unwrap();

    let signals = repo_signal::list_by_session(&pool, SESSION).await.unwrap();
    assert!(
        !signals.is_empty(),
        "ingest must produce at least one signal without the graph; got 0"
    );
    assert!(
        signals.iter().any(|s| s.detector == "tool_failure"),
        "expected a tool_failure signal from the failing cargo build; got {:?}",
        signals.iter().map(|s| s.detector.as_str()).collect::<Vec<_>>()
    );
}
