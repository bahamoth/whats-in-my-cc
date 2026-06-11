//! `diff_hunk` side-table repo. Slice-10a — schema reshaped to transcript-only
//! attribution. `introduced_by_commit_sha` removed; `introduced_by_event_id`
//! mandatory; `introduced_by_tool_use_id`, `patch_preview`, `lines_added`,
//! `lines_removed`, `user_modified` added.

use sqlx::{Row, SqlitePool};

use crate::error::Result;

#[derive(Debug, Clone)]
pub struct NewDiffHunk {
    pub diff_hunk_id: String,
    pub schema_version: String,
    pub session_id: String,
    pub file_path: String,
    pub change_type: String,
    pub line_range_after_start: Option<i64>,
    pub line_range_after_end: Option<i64>,
    pub introduced_by_event_id: String,
    pub introduced_by_tool_use_id: Option<String>,
    pub patch_preview: String,
    pub lines_added: i64,
    pub lines_removed: i64,
    pub user_modified: bool,
}

#[derive(Debug, Clone)]
pub struct DiffHunkRow {
    pub diff_hunk_id: String,
    pub schema_version: String,
    pub session_id: String,
    pub file_path: String,
    pub change_type: String,
    pub line_range_after_start: Option<i64>,
    pub line_range_after_end: Option<i64>,
    pub introduced_by_event_id: String,
    pub introduced_by_tool_use_id: Option<String>,
    pub patch_preview: String,
    pub lines_added: i64,
    pub lines_removed: i64,
    pub user_modified: bool,
}

pub async fn insert(pool: &SqlitePool, row: &NewDiffHunk) -> Result<()> {
    sqlx::query(
        "INSERT OR IGNORE INTO diff_hunk(
            diff_hunk_id, schema_version, session_id, file_path, change_type,
            line_range_after_start, line_range_after_end,
            introduced_by_event_id, introduced_by_tool_use_id,
            patch_preview, lines_added, lines_removed, user_modified)
         VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind(&row.diff_hunk_id)
    .bind(&row.schema_version)
    .bind(&row.session_id)
    .bind(&row.file_path)
    .bind(&row.change_type)
    .bind(row.line_range_after_start)
    .bind(row.line_range_after_end)
    .bind(&row.introduced_by_event_id)
    .bind(&row.introduced_by_tool_use_id)
    .bind(&row.patch_preview)
    .bind(row.lines_added)
    .bind(row.lines_removed)
    .bind(if row.user_modified { 1_i64 } else { 0_i64 })
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_session(pool: &SqlitePool, session_id: &str) -> Result<Vec<DiffHunkRow>> {
    let rows = sqlx::query(
        "SELECT diff_hunk_id, schema_version, session_id, file_path, change_type,
                line_range_after_start, line_range_after_end,
                introduced_by_event_id, introduced_by_tool_use_id,
                patch_preview, lines_added, lines_removed, user_modified
         FROM diff_hunk
         WHERE session_id = ?
         ORDER BY diff_hunk_id",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| DiffHunkRow {
            diff_hunk_id: r.get::<String, _>("diff_hunk_id"),
            schema_version: r.get::<String, _>("schema_version"),
            session_id: r.get::<String, _>("session_id"),
            file_path: r.get::<String, _>("file_path"),
            change_type: r.get::<String, _>("change_type"),
            line_range_after_start: r.get::<Option<i64>, _>("line_range_after_start"),
            line_range_after_end: r.get::<Option<i64>, _>("line_range_after_end"),
            introduced_by_event_id: r.get::<String, _>("introduced_by_event_id"),
            introduced_by_tool_use_id: r.get::<Option<String>, _>("introduced_by_tool_use_id"),
            patch_preview: r.get::<String, _>("patch_preview"),
            lines_added: r.get::<i64, _>("lines_added"),
            lines_removed: r.get::<i64, _>("lines_removed"),
            user_modified: r.get::<i64, _>("user_modified") != 0,
        })
        .collect())
}

pub async fn count_by_session(pool: &SqlitePool, session_id: &str) -> Result<i64> {
    let row = sqlx::query("SELECT COUNT(*) AS c FROM diff_hunk WHERE session_id = ?")
        .bind(session_id)
        .fetch_one(pool)
        .await?;
    Ok(row.get::<i64, _>("c"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrate;
    use sqlx::sqlite::SqlitePoolOptions;

    fn sample_new_row() -> NewDiffHunk {
        NewDiffHunk {
            diff_hunk_id: "dh_1".into(),
            schema_version: "0.4.0".into(),
            session_id: "s_real".into(),
            file_path: "src/a.rs".into(),
            change_type: "modified".into(),
            line_range_after_start: Some(42),
            line_range_after_end: Some(57),
            introduced_by_event_id: "ev_42".into(),
            introduced_by_tool_use_id: Some("toolu_xyz".into()),
            patch_preview: "@@ -42,3 +42,5 @@\n+new\n+lines\n".into(),
            lines_added: 2,
            lines_removed: 0,
            user_modified: false,
        }
    }

    #[tokio::test]
    async fn insert_then_list_session() {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        migrate(&pool).await.unwrap();
        let row = sample_new_row();
        insert(&pool, &row).await.unwrap();
        let out = list_session(&pool, "s_real").await.unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].diff_hunk_id, "dh_1");
        assert_eq!(out[0].file_path, "src/a.rs");
        assert_eq!(out[0].line_range_after_start, Some(42));
        assert_eq!(out[0].line_range_after_end, Some(57));
        assert_eq!(out[0].introduced_by_event_id, "ev_42");
        assert_eq!(
            out[0].introduced_by_tool_use_id.as_deref(),
            Some("toolu_xyz")
        );
        assert_eq!(out[0].lines_added, 2);
        assert_eq!(out[0].lines_removed, 0);
        assert!(!out[0].user_modified);
        assert_eq!(count_by_session(&pool, "s_real").await.unwrap(), 1);
    }

    #[tokio::test]
    async fn user_modified_persists_as_int() {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        migrate(&pool).await.unwrap();
        let mut row = sample_new_row();
        row.diff_hunk_id = "dh_um".into();
        row.user_modified = true;
        insert(&pool, &row).await.unwrap();
        let out = list_session(&pool, "s_real").await.unwrap();
        assert!(out[0].user_modified);
    }

    #[tokio::test]
    async fn insert_or_ignore_dedup() {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        migrate(&pool).await.unwrap();
        let row = sample_new_row();
        insert(&pool, &row).await.unwrap();
        insert(&pool, &row).await.unwrap();
        assert_eq!(count_by_session(&pool, "s_real").await.unwrap(), 1);
    }
}
