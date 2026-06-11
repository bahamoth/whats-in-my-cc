//! Slice-18 — Synthetic fixture ingest test.
//!
//! DEV-S18-04: Real secrets are NOT in fixtures. Only synthetic secrets.
//! This test asserts the gate masks each synthetic secret at ingest time.

use sqlx::sqlite::SqlitePoolOptions;
use wimcc::db::migrate;
use wimcc::ingest::store;
use wimcc::live::NoopSink;

async fn test_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    pool
}

#[tokio::test]
async fn synthetic_fixture_anthropic_key_is_redacted_at_ingest() {
    let pool = test_pool().await;
    store::ingest_file(
        &pool,
        std::path::Path::new("tests/fixtures/redaction/synthetic_secrets.jsonl"),
        &NoopSink,
    )
    .await
    .unwrap();

    let payloads: Vec<Vec<u8>> = sqlx::query_scalar("SELECT payload FROM raw_event")
        .fetch_all(&pool)
        .await
        .unwrap();

    for (i, p) in payloads.iter().enumerate() {
        let text = String::from_utf8_lossy(p);
        assert!(
            !text.contains("sk-ant-api03-SYNTHETIC_KEY_AAAABBBBCCCCDDDDEEEEFFFFGGGGHHHH"),
            "row {i}: original anthropic key must not appear in stored payload"
        );
    }
}

#[tokio::test]
async fn synthetic_fixture_pem_block_is_redacted_at_ingest() {
    let pool = test_pool().await;
    store::ingest_file(
        &pool,
        std::path::Path::new("tests/fixtures/redaction/synthetic_secrets.jsonl"),
        &NoopSink,
    )
    .await
    .unwrap();

    let payloads: Vec<Vec<u8>> = sqlx::query_scalar("SELECT payload FROM raw_event")
        .fetch_all(&pool)
        .await
        .unwrap();

    for (i, p) in payloads.iter().enumerate() {
        let text = String::from_utf8_lossy(p);
        assert!(
            !text.contains("BEGIN RSA PRIVATE KEY"),
            "row {i}: PEM header must not appear in stored payload"
        );
        assert!(
            !text.contains("FAKEPRIVATEKEY"),
            "row {i}: PEM key body must not appear in stored payload"
        );
    }
}

#[tokio::test]
async fn synthetic_fixture_email_is_redacted_at_ingest() {
    let pool = test_pool().await;
    store::ingest_file(
        &pool,
        std::path::Path::new("tests/fixtures/redaction/synthetic_secrets.jsonl"),
        &NoopSink,
    )
    .await
    .unwrap();

    let payloads: Vec<Vec<u8>> = sqlx::query_scalar("SELECT payload FROM raw_event")
        .fetch_all(&pool)
        .await
        .unwrap();

    for (i, p) in payloads.iter().enumerate() {
        let text = String::from_utf8_lossy(p);
        assert!(
            !text.contains("alice@acme.com"),
            "row {i}: original email must not appear in stored payload"
        );
    }
}
