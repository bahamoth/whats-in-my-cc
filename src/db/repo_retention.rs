//! Slice-19 — Retention tombstone repository.

use anyhow::Result;
use sqlx::SqlitePool;

/// Check whether a resource_id has a tombstone (was deleted by retention sweep).
pub async fn is_tombstoned(pool: &SqlitePool, resource_id: &str) -> Result<bool> {
    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM retention_tombstone WHERE resource_id = ?")
            .bind(resource_id)
            .fetch_one(pool)
            .await?;
    Ok(count.0 > 0)
}

/// Insert a tombstone for a deleted resource (idempotent via INSERT OR IGNORE).
pub async fn insert_tombstone(
    pool: &SqlitePool,
    resource_id: &str,
    resource_kind: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT OR IGNORE INTO retention_tombstone (resource_id, resource_kind) VALUES (?, ?)",
    )
    .bind(resource_id)
    .bind(resource_kind)
    .execute(pool)
    .await?;
    Ok(())
}
