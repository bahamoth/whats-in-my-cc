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

// Tombstone *writes* happen inside the sweep's transaction —
// see `insert_tombstone_tx` in `src/security/retention.rs`.
