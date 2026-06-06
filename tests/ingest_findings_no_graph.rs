//! Regression guard (plan task B2): ingesting a transcript must still produce
//! findings WITHOUT the graph layer.
//!
//! Before this refactor, `store::ingest_file` ran the deterministic L1 insight
//! pipeline as a side effect of the graph rebuild. After the graph layer is
//! deleted, ingest calls `insight::pipeline::run_extractors` directly. This
//! test pins that behaviour: ingest a real-shaped transcript that contains one
//! genuine (user_visible) Bash `cargo build` failure and assert that
//! `repo_finding::list_by_session` returns a non-empty set.
//!
//! Fixture payload shape (tool_use / tool_result with `is_error`) is the frozen
//! real transcript shape verified against real fixture
//! aac68973-729e-4014-a02b-28a556f5ff29 (see `tests/extractor_tool_failure.rs`).

use sqlx::sqlite::SqlitePoolOptions;
use wimcc::db::{migrate, repo_finding};
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
async fn ingest_produces_findings_without_graph() {
    let pool = make_pool().await;
    store::ingest_file(&pool, std::path::Path::new(FIXTURE), &NoopSink)
        .await
        .unwrap();

    let findings = repo_finding::list_by_session(&pool, SESSION).await.unwrap();
    assert!(
        !findings.is_empty(),
        "ingest must produce at least one finding without the graph; got 0"
    );
    assert!(
        findings.iter().any(|f| f.category == "tool_failure"),
        "expected a tool_failure finding from the failing cargo build; got {:?}",
        findings.iter().map(|f| f.category.as_str()).collect::<Vec<_>>()
    );
}
