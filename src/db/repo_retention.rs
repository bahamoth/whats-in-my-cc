//! Slice-19 — Retention tombstone repository.

use anyhow::Result;
use sqlx::SqlitePool;

/// Check whether a resource has a tombstone (was deleted by retention sweep).
/// Kind-scoped: a tombstone of another class sharing the id must not match —
/// session ids are caller-supplied, so cross-class collisions are possible.
pub async fn is_tombstoned(
    pool: &SqlitePool,
    resource_id: &str,
    resource_kind: &str,
) -> Result<bool> {
    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM retention_tombstone WHERE resource_id = ? AND resource_kind = ?",
    )
    .bind(resource_id)
    .bind(resource_kind)
    .fetch_one(pool)
    .await?;
    Ok(count.0 > 0)
}

// Tombstone *writes* happen inside the sweep's transaction —
// see `insert_tombstone_tx` in `src/security/retention.rs`.
