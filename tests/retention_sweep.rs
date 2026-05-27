//! Slice-19 — Red-locking tests: retention sweep.
//!
//! These tests will FAIL until `src/security/retention.rs` and
//! migrations 0012 + 0013 are implemented.

use witmcc::security::retention::{Profile, RetentionPolicy};

async fn test_pool() -> sqlx::SqlitePool {
    let pool = witmcc::db::connect(":memory:").await.unwrap();
    witmcc::db::migrate(&pool).await.unwrap();
    pool
}

/// Seed a raw_event with `captured_at` set to `days_old` days in the past.
/// Returns the raw_event_id.
async fn seed_old_raw_event(pool: &sqlx::SqlitePool, days_old: i64) -> String {
    // First ensure an ingest_run exists (raw_event has FK on ingest_run).
    let run_id = format!("run_{days_old}d_{}", ulid::Ulid::new());
    sqlx::query(
        "INSERT OR IGNORE INTO ingest_run (run_id, started_at, status) VALUES (?, datetime('now'), 'done')"
    )
    .bind(&run_id)
    .execute(pool)
    .await
    .unwrap();

    let id = format!("raw_{days_old}d_{}", ulid::Ulid::new());
    let sha = format!("sha_{id}");
    sqlx::query(
        "INSERT INTO raw_event (raw_event_id, ingest_run_id, source_type, source_uri, source_line_no, source_byte_offset, payload_sha256, payload, captured_at)
         VALUES (?, ?, 'claude_transcript', 'test.jsonl', 0, 0, ?, '{}', datetime('now', ?))"
    )
    .bind(&id)
    .bind(&run_id)
    .bind(&sha)
    .bind(format!("-{days_old} days"))
    .execute(pool)
    .await
    .unwrap();
    id
}

#[tokio::test]
async fn sweep_default_profile_deletes_raw_older_than_30d() {
    let pool = test_pool().await;
    seed_old_raw_event(&pool, 31).await;
    seed_old_raw_event(&pool, 5).await; // should NOT be deleted

    let p = RetentionPolicy { profile: Profile::Default };
    let report = witmcc::security::retention::run_sweep(&pool, &p).await.unwrap();

    assert_eq!(
        report.deletions.get("raw_event").copied().unwrap_or(0),
        1,
        "should delete exactly 1 raw_event older than 30d"
    );

    let remaining: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM raw_event")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(remaining, 1, "1 recent raw_event should remain");
}

#[tokio::test]
async fn sweep_none_profile_deletes_nothing() {
    let pool = test_pool().await;
    seed_old_raw_event(&pool, 365).await;
    seed_old_raw_event(&pool, 400).await;

    let p = RetentionPolicy { profile: Profile::None };
    let report = witmcc::security::retention::run_sweep(&pool, &p).await.unwrap();

    let total_deleted: u64 = report.deletions.values().sum();
    assert_eq!(total_deleted, 0, "none profile should delete nothing");
}

#[tokio::test]
async fn sweep_strict_profile_deletes_raw_older_than_7d() {
    let pool = test_pool().await;
    seed_old_raw_event(&pool, 8).await;  // should be deleted (strict = 7d)
    seed_old_raw_event(&pool, 3).await;  // should NOT be deleted

    let p = RetentionPolicy { profile: Profile::Strict };
    let report = witmcc::security::retention::run_sweep(&pool, &p).await.unwrap();

    assert_eq!(
        report.deletions.get("raw_event").copied().unwrap_or(0),
        1,
        "strict profile should delete raw_events older than 7d"
    );
}

#[tokio::test]
async fn deleted_resource_has_tombstone() {
    let pool = test_pool().await;
    let id = seed_old_raw_event(&pool, 31).await;

    let p = RetentionPolicy { profile: Profile::Default };
    witmcc::security::retention::run_sweep(&pool, &p).await.unwrap();

    let tomb: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM retention_tombstone WHERE resource_id = ?",
    )
    .bind(&id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(tomb, 1, "deleted resource should have a tombstone row");
}

#[tokio::test]
async fn sweep_writes_audit_row() {
    let pool = test_pool().await;
    seed_old_raw_event(&pool, 31).await;

    let p = RetentionPolicy { profile: Profile::Default };
    witmcc::security::retention::run_sweep(&pool, &p).await.unwrap();

    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit WHERE event = 'retention.deleted'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(n, 1, "sweep should write exactly one audit row per run");
}
