//! Slice-11 — `finding` table repo. Insert-only (idempotent on PK conflict)
//! + per-session list + per-session delete (used by rule re-run cycle).
//!
//! Per spec §08, `evidence_refs` is an array of `{node_id, role}` objects.
//! We store it as a JSON string column and surface a `serde_json::Value` so
//! callers don't depend on a fixed struct shape — different rules may attach
//! additional metadata (e.g. `counter_evidence_refs`) without a migration.

use sqlx::{Row, SqlitePool};

use crate::error::Result;

#[derive(Debug, Clone)]
pub struct NewFinding {
    pub finding_id: String,
    pub schema_version: String,
    pub session_id: String,
    pub category: String,
    pub severity: String,
    pub claim: String,
    pub confidence: f64,
    pub limitations: serde_json::Value,
    pub evidence_refs: serde_json::Value,
    pub generated_at: String,
    pub rule_version: String,
}

#[derive(Debug, Clone)]
pub struct FindingRow {
    pub finding_id: String,
    pub schema_version: String,
    pub session_id: String,
    pub category: String,
    pub severity: String,
    pub claim: String,
    pub confidence: f64,
    pub limitations: serde_json::Value,
    pub evidence_refs: serde_json::Value,
    pub generated_at: String,
    pub rule_version: String,
}

pub async fn insert(pool: &SqlitePool, row: &NewFinding) -> Result<()> {
    sqlx::query(
        "INSERT OR IGNORE INTO finding(
            finding_id, schema_version, session_id, category, severity,
            claim, confidence, limitations_json, evidence_refs_json,
            generated_at, rule_version)
         VALUES(?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind(&row.finding_id)
    .bind(&row.schema_version)
    .bind(&row.session_id)
    .bind(&row.category)
    .bind(&row.severity)
    .bind(&row.claim)
    .bind(row.confidence)
    .bind(row.limitations.to_string())
    .bind(row.evidence_refs.to_string())
    .bind(&row.generated_at)
    .bind(&row.rule_version)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_session(pool: &SqlitePool, session_id: &str) -> Result<Vec<FindingRow>> {
    let rows = sqlx::query(
        "SELECT finding_id, schema_version, session_id, category, severity,
                claim, confidence, limitations_json, evidence_refs_json,
                generated_at, rule_version
         FROM finding
         WHERE session_id = ?
         ORDER BY generated_at ASC, finding_id ASC",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(row_to_finding).collect())
}

pub async fn delete_session(pool: &SqlitePool, session_id: &str) -> Result<()> {
    sqlx::query("DELETE FROM finding WHERE session_id = ?")
        .bind(session_id)
        .execute(pool)
        .await?;
    Ok(())
}

fn row_to_finding(r: sqlx::sqlite::SqliteRow) -> FindingRow {
    let lim_s: String = r.get("limitations_json");
    let ev_s: String = r.get("evidence_refs_json");
    FindingRow {
        finding_id: r.get("finding_id"),
        schema_version: r.get("schema_version"),
        session_id: r.get("session_id"),
        category: r.get("category"),
        severity: r.get("severity"),
        claim: r.get("claim"),
        confidence: r.get("confidence"),
        limitations: serde_json::from_str(&lim_s).unwrap_or(serde_json::Value::Array(vec![])),
        evidence_refs: serde_json::from_str(&ev_s).unwrap_or(serde_json::Value::Array(vec![])),
        generated_at: r.get("generated_at"),
        rule_version: r.get("rule_version"),
    }
}
