//! `verification_run` side-table repo (slice-11).
//!
//! Mirrors the shape of `repo_diff_hunk`: insert, list_session, get.
//! The graph builder reads from this repo via `list_session` to materialise
//! `verification_run` nodes and `triggered_verification` / `covers_diff_hunk`
//! edges.

use sqlx::{Row, SqlitePool};

use crate::error::Result;

/// Row returned by `list_session` / `get`. All columns are present.
#[derive(Debug, Clone, Default)]
pub struct VerificationRunRow {
    pub verification_run_id: String,
    pub schema_version: String,
    pub session_id: String,
    pub source: String,
    pub command: String,
    pub command_kind: String,
    pub trigger_event_id: String,
    pub trigger_tool_use_id: Option<String>,
    pub status: String,
    pub detection_basis: String,
    pub status_basis: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub exit_code: Option<i32>,
    pub failure_summary: Option<String>,
    pub raw_event_id: String,
    pub parser_version: String,
}

/// Row type for insertion (same as `VerificationRunRow` but keeps the caller
/// explicit about which fields are supplied vs. defaulted).
pub type NewVerificationRun = VerificationRunRow;

/// Insert a row. Uses `INSERT OR REPLACE` so re-ingesting the same trigger
/// event produces at most one row (idempotent, last-writer-wins per spec §7).
pub async fn insert(pool: &SqlitePool, row: &NewVerificationRun) -> Result<()> {
    sqlx::query(
        "INSERT OR REPLACE INTO verification_run(
            verification_run_id, schema_version, session_id, source, command,
            command_kind, trigger_event_id, trigger_tool_use_id, status,
            started_at, ended_at, exit_code, failure_summary,
            raw_event_id, parser_version, detection_basis, status_basis)
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind(&row.verification_run_id)
    .bind(&row.schema_version)
    .bind(&row.session_id)
    .bind(&row.source)
    .bind(&row.command)
    .bind(&row.command_kind)
    .bind(&row.trigger_event_id)
    .bind(&row.trigger_tool_use_id)
    .bind(&row.status)
    .bind(&row.started_at)
    .bind(&row.ended_at)
    .bind(row.exit_code.map(|x| x as i64))
    .bind(&row.failure_summary)
    .bind(&row.raw_event_id)
    .bind(&row.parser_version)
    .bind(&row.detection_basis)
    .bind(&row.status_basis)
    .execute(pool)
    .await?;
    Ok(())
}

/// List all verification runs for a session, ordered by `started_at`.
pub async fn list_session(
    pool: &SqlitePool,
    session_id: &str,
) -> Result<Vec<VerificationRunRow>> {
    let rows = sqlx::query(
        "SELECT verification_run_id, schema_version, session_id, source, command,
                command_kind, trigger_event_id, trigger_tool_use_id, status,
                started_at, ended_at, exit_code, failure_summary,
                raw_event_id, parser_version, detection_basis, status_basis
         FROM verification_run
         WHERE session_id = ?
         ORDER BY started_at, verification_run_id",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(map_row).collect())
}

/// Fetch a single row by ID. Returns `None` if not found.
pub async fn get(
    pool: &SqlitePool,
    verification_run_id: &str,
) -> Result<Option<VerificationRunRow>> {
    let row = sqlx::query(
        "SELECT verification_run_id, schema_version, session_id, source, command,
                command_kind, trigger_event_id, trigger_tool_use_id, status,
                started_at, ended_at, exit_code, failure_summary,
                raw_event_id, parser_version, detection_basis, status_basis
         FROM verification_run
         WHERE verification_run_id = ?",
    )
    .bind(verification_run_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(map_row))
}

fn map_row(r: sqlx::sqlite::SqliteRow) -> VerificationRunRow {
    VerificationRunRow {
        verification_run_id: r.get("verification_run_id"),
        schema_version: r.get("schema_version"),
        session_id: r.get("session_id"),
        source: r.get("source"),
        command: r.get("command"),
        command_kind: r.get("command_kind"),
        trigger_event_id: r.get("trigger_event_id"),
        trigger_tool_use_id: r.get("trigger_tool_use_id"),
        status: r.get("status"),
        detection_basis: r.get("detection_basis"),
        status_basis: r.get("status_basis"),
        started_at: r.get("started_at"),
        ended_at: r.get("ended_at"),
        exit_code: r.get::<Option<i64>, _>("exit_code").map(|x| x as i32),
        failure_summary: r.get("failure_summary"),
        raw_event_id: r.get("raw_event_id"),
        parser_version: r.get("parser_version"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrate;
    use sqlx::sqlite::SqlitePoolOptions;

    fn sample_row() -> NewVerificationRun {
        NewVerificationRun {
            verification_run_id: "vr_test_001".into(),
            schema_version: "verification_run.v1".into(),
            session_id: "sess_vr_test".into(),
            source: "bash".into(),
            command: "cargo test".into(),
            command_kind: "test_suite_rust".into(),
            trigger_event_id: "ev_001".into(),
            trigger_tool_use_id: Some("toolu_001".into()),
            status: "passed".into(),
            detection_basis: "known_tool".into(),
            status_basis: "exit".into(),
            started_at: "2026-05-27T10:00:00Z".into(),
            ended_at: Some("2026-05-27T10:00:05Z".into()),
            exit_code: Some(0),
            failure_summary: None,
            raw_event_id: "raw_001".into(),
            parser_version: "verification_run@v1".into(),
        }
    }

    #[tokio::test]
    async fn insert_then_list_session() {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        migrate(&pool).await.unwrap();
        let row = sample_row();
        insert(&pool, &row).await.unwrap();
        let out = list_session(&pool, "sess_vr_test").await.unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].verification_run_id, "vr_test_001");
        assert_eq!(out[0].command, "cargo test");
        assert_eq!(out[0].status, "passed");
        assert_eq!(out[0].detection_basis, "known_tool");
        assert_eq!(out[0].status_basis, "exit");
        assert_eq!(out[0].exit_code, Some(0));
        assert_eq!(out[0].trigger_tool_use_id.as_deref(), Some("toolu_001"));
    }

    #[tokio::test]
    async fn get_returns_correct_row() {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        migrate(&pool).await.unwrap();
        let row = sample_row();
        insert(&pool, &row).await.unwrap();
        let fetched = get(&pool, "vr_test_001").await.unwrap().unwrap();
        assert_eq!(fetched.command_kind, "test_suite_rust");
        assert_eq!(fetched.source, "bash");
    }

    #[tokio::test]
    async fn get_returns_none_for_missing() {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        migrate(&pool).await.unwrap();
        let result = get(&pool, "vr_nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn insert_or_replace_dedup() {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        migrate(&pool).await.unwrap();
        let row = sample_row();
        insert(&pool, &row).await.unwrap();
        insert(&pool, &row).await.unwrap();
        let out = list_session(&pool, "sess_vr_test").await.unwrap();
        assert_eq!(out.len(), 1, "INSERT OR REPLACE must deduplicate by PK");
    }
}
