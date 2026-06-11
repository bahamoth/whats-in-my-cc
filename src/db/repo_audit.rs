//! Slice-19 — Audit row repository.

use anyhow::Result;
use sqlx::SqlitePool;

/// A single audit row.
pub struct AuditRow {
    pub audit_id: String,
    pub event: String,
    pub actor: Option<String>,
    pub payload: String,
    pub created_at: String,
}

/// List the most recent `limit` audit rows, ordered by created_at DESC.
pub async fn list_recent(pool: &SqlitePool, limit: i64) -> Result<Vec<AuditRow>> {
    let rows: Vec<(String, String, Option<String>, String, String)> = sqlx::query_as(
        "SELECT audit_id, event, actor, payload, created_at
           FROM audit
           ORDER BY created_at DESC
           LIMIT ?",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(audit_id, event, actor, payload, created_at)| AuditRow {
            audit_id,
            event,
            actor,
            payload,
            created_at,
        })
        .collect())
}

/// Insert an audit row.
pub async fn insert(
    pool: &SqlitePool,
    audit_id: &str,
    event: &str,
    actor: Option<&str>,
    payload: &str,
) -> Result<()> {
    sqlx::query("INSERT INTO audit (audit_id, event, actor, payload) VALUES (?, ?, ?, ?)")
        .bind(audit_id)
        .bind(event)
        .bind(actor)
        .bind(payload)
        .execute(pool)
        .await?;
    Ok(())
}
