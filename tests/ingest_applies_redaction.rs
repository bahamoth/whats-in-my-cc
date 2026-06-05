//! Slice-18 — Ingest wiring: store_raw_event must apply the redaction gate
//! before persisting payload and must write a redaction_manifest.

use sqlx::sqlite::SqlitePoolOptions;
use wimcc::db::migrate;

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
async fn ingest_masks_anthropic_key_in_stored_payload() {
    use wimcc::live::NoopSink;
    use wimcc::ingest::store;

    let pool = test_pool().await;
    // Use the synthetic fixture which contains sk-ant-api03-SYNTHETIC_KEY_...
    store::ingest_file(
        &pool,
        std::path::Path::new("tests/fixtures/redaction/synthetic_secrets.jsonl"),
        &NoopSink,
    )
    .await
    .unwrap();

    // Verify no raw_event payload contains the original key suffix
    let payloads: Vec<Vec<u8>> =
        sqlx::query_scalar("SELECT payload FROM raw_event")
            .fetch_all(&pool)
            .await
            .unwrap();

    for p in &payloads {
        let text = String::from_utf8_lossy(p);
        assert!(
            !text.contains("SYNTHETIC_KEY_AAAABBBBCCCCDDDDEEEEFFFFGGGGHHHH"),
            "stored payload must not contain original anthropic key"
        );
    }
}

#[tokio::test]
async fn ingest_masks_private_key_pem_in_stored_payload() {
    use wimcc::live::NoopSink;
    use wimcc::ingest::store;

    let pool = test_pool().await;
    store::ingest_file(
        &pool,
        std::path::Path::new("tests/fixtures/redaction/synthetic_secrets.jsonl"),
        &NoopSink,
    )
    .await
    .unwrap();

    let payloads: Vec<Vec<u8>> =
        sqlx::query_scalar("SELECT payload FROM raw_event")
            .fetch_all(&pool)
            .await
            .unwrap();

    for p in &payloads {
        let text = String::from_utf8_lossy(p);
        assert!(
            !text.contains("BEGIN RSA PRIVATE KEY"),
            "stored payload must not contain raw PEM block header"
        );
    }
}

#[tokio::test]
async fn ingest_writes_redaction_manifest_column() {
    use wimcc::live::NoopSink;
    use wimcc::ingest::store;

    let pool = test_pool().await;
    store::ingest_file(
        &pool,
        std::path::Path::new("tests/fixtures/redaction/synthetic_secrets.jsonl"),
        &NoopSink,
    )
    .await
    .unwrap();

    // At least one row must have a non-NULL redaction_manifest
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM raw_event WHERE redaction_manifest IS NOT NULL",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert!(
        count >= 1,
        "at least 1 raw_event must have a non-NULL redaction_manifest after ingesting secrets fixture"
    );
}

#[tokio::test]
async fn ingest_sets_redaction_state_to_redacted_when_secret_found() {
    use wimcc::live::NoopSink;
    use wimcc::ingest::store;

    let pool = test_pool().await;
    store::ingest_file(
        &pool,
        std::path::Path::new("tests/fixtures/redaction/synthetic_secrets.jsonl"),
        &NoopSink,
    )
    .await
    .unwrap();

    // No row should have the old placeholder "unredacted"
    let unredacted_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM raw_event WHERE redaction_state = 'unredacted'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        unredacted_count, 0,
        "no raw_event must have old placeholder redaction_state='unredacted' after slice-18"
    );
}
