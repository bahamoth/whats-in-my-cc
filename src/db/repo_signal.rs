//! `signal` side-table repo.
//!
//! Signals are the output of deterministic detectors (Plan 1: finding → signal).
//! `signal_id` is derived deterministically so `INSERT OR REPLACE` deduplicates
//! re-runs (idempotent by primary key) — provided the id is stable. Aggregating
//! detectors derive it from a stable `dedup_key`; the pipeline additionally
//! `reconcile`s each (session, detector) to drop rows orphaned by a changed id
//! (dogfooding fix 2026-06-11). NO severity/confidence — those are judgments;
//! only facts are stored (spec §6.3).

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

/// Reconcile a `(session_id, detector)` group to the current pass: delete every
/// stored signal for that group whose `signal_id` is NOT in `keep_ids`.
///
/// `run_detectors` loads the full session view, so the current pass is the
/// complete authoritative set for each detector. This removes stale signals that
/// `INSERT OR REPLACE` cannot reach — e.g. an aggregating detector (re_read)
/// whose evidence grew across re-ingests and thus changed `signal_id`, leaving
/// the old row orphaned (dogfooding regression 2026-06-11). An empty `keep_ids`
/// means the detector produced nothing this pass → delete all its signals.
pub async fn reconcile(
    pool: &SqlitePool,
    session_id: &str,
    detector: &str,
    keep_ids: &[String],
) -> Result<()> {
    // Build a parameterized NOT IN (...) list to avoid SQL injection / quoting.
    let placeholders = if keep_ids.is_empty() {
        // No survivors: delete all rows for this (session, detector).
        String::new()
    } else {
        let marks = vec!["?"; keep_ids.len()].join(",");
        format!(" AND signal_id NOT IN ({marks})")
    };
    let sql = format!("DELETE FROM signal WHERE session_id=? AND detector=?{placeholders}");
    let mut q = sqlx::query(&sql).bind(session_id).bind(detector);
    for id in keep_ids {
        q = q.bind(id);
    }
    q.execute(pool).await?;
    Ok(())
}

/// List all signals for a session (ordered by created_at DESC).
pub async fn list_by_session(pool: &SqlitePool, session_id: &str) -> Result<Vec<SignalRow>> {
    let rows = sqlx::query("SELECT * FROM signal WHERE session_id=? ORDER BY created_at DESC")
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
