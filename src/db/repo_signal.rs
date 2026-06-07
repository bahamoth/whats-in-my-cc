//! `signal` side-table repo.
//!
//! Signals are the output of deterministic detectors (Plan 1: finding → signal).
//! `signal_id` is derived deterministically so `INSERT OR REPLACE` safely
//! deduplicates re-runs (idempotent by primary key). NO severity/confidence —
//! those are judgments; only facts are stored (spec §6.3).

use sqlx::{Row, SqlitePool};

use crate::error::Result;

/// A row in the `signal` table.
#[derive(Debug, Clone)]
pub struct SignalRow {
    pub signal_id: String,
    pub schema_version: String,
    pub session_id: String,
    pub detector: String,
    /// Optional signal sub-type (e.g. tool_failure failure class). NULL-able.
    pub subkind: Option<String>,
    pub summary: String,
    /// JSON array of event_id strings.
    pub evidence_refs: String,
    /// JSON object — detector-specific facts (no severity/confidence).
    pub facts: String,
    /// JSON object: `{ detector, layer }`.
    pub provenance: String,
    pub created_at: String,
}

/// Insert or replace a signal (idempotent by `signal_id`).
pub async fn insert(pool: &SqlitePool, row: &SignalRow) -> Result<()> {
    sqlx::query(
        "INSERT OR REPLACE INTO signal \
         (signal_id, schema_version, session_id, detector, subkind, summary, \
          evidence_refs, facts, provenance, created_at) \
         VALUES (?,?,?,?,?,?,?,?,?,?)",
    )
    .bind(&row.signal_id)
    .bind(&row.schema_version)
    .bind(&row.session_id)
    .bind(&row.detector)
    .bind(&row.subkind)
    .bind(&row.summary)
    .bind(&row.evidence_refs)
    .bind(&row.facts)
    .bind(&row.provenance)
    .bind(&row.created_at)
    .execute(pool)
    .await?;
    Ok(())
}

/// List all signals for a session (ordered by created_at DESC).
pub async fn list_by_session(pool: &SqlitePool, session_id: &str) -> Result<Vec<SignalRow>> {
    let rows =
        sqlx::query("SELECT * FROM signal WHERE session_id=? ORDER BY created_at DESC")
            .bind(session_id)
            .fetch_all(pool)
            .await?;
    Ok(rows.into_iter().map(map_row).collect())
}

/// Fetch a single signal by ID (returns None if not found).
pub async fn get(pool: &SqlitePool, signal_id: &str) -> Result<Option<SignalRow>> {
    let row = sqlx::query("SELECT * FROM signal WHERE signal_id=?")
        .bind(signal_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(map_row))
}

fn map_row(r: sqlx::sqlite::SqliteRow) -> SignalRow {
    SignalRow {
        signal_id: r.get("signal_id"),
        schema_version: r.get("schema_version"),
        session_id: r.get("session_id"),
        detector: r.get("detector"),
        subkind: r.get("subkind"),
        summary: r.get("summary"),
        evidence_refs: r.get("evidence_refs"),
        facts: r.get("facts"),
        provenance: r.get("provenance"),
        created_at: r.get("created_at"),
    }
}
