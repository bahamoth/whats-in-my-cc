//! `findings_pending_judge` side-table repo (slice-15).
//!
//! Candidates that need judge evaluation but couldn't proceed (budget, disabled, transport error).
//! Drained by the pipeline at the start of the next rebuild if budget is available.
//! Monotone progress guarantee: candidates are never silently dropped.

use sqlx::{Row, SqlitePool};

use crate::error::Result;

/// The insertable fields of a pending finding candidate.
/// DB-generated columns (`queued_at`, `last_attempt_at`, `attempts`,
/// `schema_version` default) are omitted from inserts.
#[derive(Debug, Clone)]
pub struct PendingFindingRow {
    pub candidate_id: String,
    pub schema_version: String,
    pub session_id: String,
    pub category: String,
    pub confidence_l1: f32,
    pub evidence_refs: String,       // JSON array
    pub evidence_projection: String, // JSON object
}

/// A full row as read back from the DB (includes DB-generated fields).
#[derive(Debug, Clone)]
pub struct PendingFindingRecord {
    pub candidate_id: String,
    pub schema_version: String,
    pub session_id: String,
    pub category: String,
    pub confidence_l1: f64,
    pub evidence_refs: String,
    pub evidence_projection: String,
    pub queued_at: String,
    pub last_attempt_at: Option<String>,
    pub attempts: i64,
}

/// Enqueue a candidate (INSERT OR IGNORE — idempotent by candidate_id).
/// Uses INSERT OR IGNORE so a candidate already in the queue is not overwritten
/// (preserves `queued_at` and `attempts`).
pub async fn enqueue(pool: &SqlitePool, row: &PendingFindingRow) -> Result<()> {
    sqlx::query(
        "INSERT OR IGNORE INTO findings_pending_judge \
         (candidate_id, session_id, category, confidence_l1, evidence_refs, evidence_projection) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&row.candidate_id)
    .bind(&row.session_id)
    .bind(&row.category)
    .bind(row.confidence_l1 as f64)
    .bind(&row.evidence_refs)
    .bind(&row.evidence_projection)
    .execute(pool)
    .await?;
    Ok(())
}

/// Load all pending candidates for a session, ordered by queued_at ascending.
pub async fn list_session(
    pool: &SqlitePool,
    session_id: &str,
) -> Result<Vec<PendingFindingRecord>> {
    let rows = sqlx::query(
        "SELECT candidate_id, schema_version, session_id, category, \
         confidence_l1, evidence_refs, evidence_projection, \
         queued_at, last_attempt_at, attempts \
         FROM findings_pending_judge \
         WHERE session_id = ? \
         ORDER BY queued_at ASC",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| PendingFindingRecord {
            candidate_id: r.get("candidate_id"),
            schema_version: r.get("schema_version"),
            session_id: r.get("session_id"),
            category: r.get("category"),
            confidence_l1: r.get("confidence_l1"),
            evidence_refs: r.get("evidence_refs"),
            evidence_projection: r.get("evidence_projection"),
            queued_at: r.get("queued_at"),
            last_attempt_at: r.try_get("last_attempt_at").ok(),
            attempts: r.get("attempts"),
        })
        .collect())
}

/// Remove a candidate after it has been judged (promoted or discarded).
pub async fn dequeue(pool: &SqlitePool, candidate_id: &str) -> Result<()> {
    sqlx::query("DELETE FROM findings_pending_judge WHERE candidate_id = ?")
        .bind(candidate_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Total count of pending candidates (all sessions) — used for health endpoint.
pub async fn count_all(pool: &SqlitePool) -> Result<i64> {
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM findings_pending_judge")
        .fetch_one(pool)
        .await?;
    Ok(n)
}

/// Bump `attempts` and update `last_attempt_at` to now.
pub async fn record_attempt(pool: &SqlitePool, candidate_id: &str) -> Result<()> {
    sqlx::query(
        "UPDATE findings_pending_judge \
         SET attempts = attempts + 1, last_attempt_at = datetime('now') \
         WHERE candidate_id = ?",
    )
    .bind(candidate_id)
    .execute(pool)
    .await?;
    Ok(())
}
