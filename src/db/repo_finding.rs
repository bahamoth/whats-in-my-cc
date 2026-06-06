//! `finding` side-table repo (slice-14).
//!
//! Findings are the output of the L1 deterministic extractor pipeline.
//! `finding_id` is derived deterministically so `INSERT OR REPLACE` safely
//! deduplicates re-runs (idempotent by primary key).

use sqlx::{Row, SqlitePool};

use crate::error::Result;

/// A row in the `finding` table.
#[derive(Debug, Clone)]
pub struct FindingRow {
    pub finding_id: String,
    pub schema_version: String,
    pub session_id: String,
    pub category: String,
    /// Optional finding sub-type (e.g. tool_failure failure class). NULL-able.
    pub subkind: Option<String>,
    pub severity: String,
    pub confidence: f64,
    pub summary: String,
    /// JSON array of event_id strings.
    pub evidence_refs: String,
    /// JSON object — the L1-side evidence projection.
    pub evidence_projection: String,
    /// JSON object: `{ extractor, layer, judge, judge_template_version, rule_pack }`.
    pub provenance: String,
    /// `"active"` | `"discarded"`. (All L1 findings are `"active"`; the judge
    /// layer and its `"pending_judge"` status were removed — see #judge-removal.)
    pub status: String,
    pub created_at: String,
}

/// Insert or replace a finding (idempotent by `finding_id`).
pub async fn insert(pool: &SqlitePool, row: &FindingRow) -> Result<()> {
    sqlx::query(
        "INSERT OR REPLACE INTO finding \
         (finding_id, schema_version, session_id, category, subkind, severity, confidence, \
          summary, evidence_refs, evidence_projection, provenance, status) \
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind(&row.finding_id)
    .bind(&row.schema_version)
    .bind(&row.session_id)
    .bind(&row.category)
    .bind(&row.subkind)
    .bind(&row.severity)
    .bind(row.confidence)
    .bind(&row.summary)
    .bind(&row.evidence_refs)
    .bind(&row.evidence_projection)
    .bind(&row.provenance)
    .bind(&row.status)
    .execute(pool)
    .await?;
    Ok(())
}

/// Query parameters for listing findings.
#[derive(Debug, Default, Clone)]
pub struct ListFilter {
    pub session_id: Option<String>,
    pub category: Option<String>,
    pub severity: Option<String>,
    pub status: Option<String>,
    pub subkind: Option<String>,
    pub limit: i64,
}

/// List findings according to the filter. Defaults to `status = "active"`.
pub async fn list(pool: &SqlitePool, f: &ListFilter) -> Result<Vec<FindingRow>> {
    let status = f.status.as_deref().unwrap_or("active");
    let limit = f.limit.max(1).min(200);

    // Build a dynamic but safe query using explicit branches.
    let rows = match (&f.session_id, &f.category, &f.severity) {
        (Some(sid), Some(cat), Some(sev)) => {
            sqlx::query(
                "SELECT * FROM finding \
                 WHERE session_id=? AND category=? AND severity=? AND status=? \
                 ORDER BY created_at DESC LIMIT ?",
            )
            .bind(sid)
            .bind(cat)
            .bind(sev)
            .bind(status)
            .bind(limit)
            .fetch_all(pool)
            .await?
        }
        (Some(sid), Some(cat), None) => {
            sqlx::query(
                "SELECT * FROM finding \
                 WHERE session_id=? AND category=? AND status=? \
                 ORDER BY created_at DESC LIMIT ?",
            )
            .bind(sid)
            .bind(cat)
            .bind(status)
            .bind(limit)
            .fetch_all(pool)
            .await?
        }
        (Some(sid), None, Some(sev)) => {
            sqlx::query(
                "SELECT * FROM finding \
                 WHERE session_id=? AND severity=? AND status=? \
                 ORDER BY created_at DESC LIMIT ?",
            )
            .bind(sid)
            .bind(sev)
            .bind(status)
            .bind(limit)
            .fetch_all(pool)
            .await?
        }
        (Some(sid), None, None) => {
            sqlx::query(
                "SELECT * FROM finding \
                 WHERE session_id=? AND status=? \
                 ORDER BY created_at DESC LIMIT ?",
            )
            .bind(sid)
            .bind(status)
            .bind(limit)
            .fetch_all(pool)
            .await?
        }
        (None, Some(cat), Some(sev)) => {
            sqlx::query(
                "SELECT * FROM finding \
                 WHERE category=? AND severity=? AND status=? \
                 ORDER BY created_at DESC LIMIT ?",
            )
            .bind(cat)
            .bind(sev)
            .bind(status)
            .bind(limit)
            .fetch_all(pool)
            .await?
        }
        (None, Some(cat), None) => {
            sqlx::query(
                "SELECT * FROM finding \
                 WHERE category=? AND status=? \
                 ORDER BY created_at DESC LIMIT ?",
            )
            .bind(cat)
            .bind(status)
            .bind(limit)
            .fetch_all(pool)
            .await?
        }
        (None, None, Some(sev)) => {
            sqlx::query(
                "SELECT * FROM finding \
                 WHERE severity=? AND status=? \
                 ORDER BY created_at DESC LIMIT ?",
            )
            .bind(sev)
            .bind(status)
            .bind(limit)
            .fetch_all(pool)
            .await?
        }
        (None, None, None) => {
            sqlx::query(
                "SELECT * FROM finding \
                 WHERE status=? \
                 ORDER BY created_at DESC LIMIT ?",
            )
            .bind(status)
            .bind(limit)
            .fetch_all(pool)
            .await?
        }
    };

    let mut out: Vec<FindingRow> = rows.into_iter().map(map_row).collect();
    if let Some(sk) = &f.subkind {
        out.retain(|r| r.subkind.as_deref() == Some(sk.as_str()));
    }
    Ok(out)
}

/// Fetch a single finding by ID (returns None if not found).
pub async fn get(pool: &SqlitePool, finding_id: &str) -> Result<Option<FindingRow>> {
    let row = sqlx::query("SELECT * FROM finding WHERE finding_id=?")
        .bind(finding_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(map_row))
}

/// List all findings for a session (all statuses, ordered by created_at).
pub async fn list_by_session(pool: &SqlitePool, session_id: &str) -> Result<Vec<FindingRow>> {
    let rows = sqlx::query(
        "SELECT * FROM finding WHERE session_id=? ORDER BY created_at DESC",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(map_row).collect())
}

/// Count active findings for a session+category grouped by subkind.
/// Returns rows of `(subkind_or_null, count)`. Used by the tool-failure
/// summary endpoint so the surface can show user-visible-only counts.
pub async fn count_by_subkind(
    pool: &SqlitePool,
    session_id: &str,
    category: &str,
) -> Result<Vec<(Option<String>, i64)>> {
    let rows = sqlx::query(
        "SELECT subkind AS subkind, COUNT(*) AS n \
         FROM finding \
         WHERE session_id=? AND category=? AND status='active' \
         GROUP BY subkind",
    )
    .bind(session_id)
    .bind(category)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.get::<Option<String>, _>("subkind"), r.get::<i64, _>("n")))
        .collect())
}

fn map_row(r: sqlx::sqlite::SqliteRow) -> FindingRow {
    FindingRow {
        finding_id: r.get("finding_id"),
        schema_version: r.get("schema_version"),
        session_id: r.get("session_id"),
        category: r.get("category"),
        subkind: r.get("subkind"),
        severity: r.get("severity"),
        confidence: r.get("confidence"),
        summary: r.get("summary"),
        evidence_refs: r.get("evidence_refs"),
        evidence_projection: r.get("evidence_projection"),
        provenance: r.get("provenance"),
        status: r.get("status"),
        created_at: r.get("created_at"),
    }
}
