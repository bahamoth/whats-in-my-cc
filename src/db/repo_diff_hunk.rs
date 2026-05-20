use sqlx::{Row, SqlitePool};

use crate::error::Result;

#[derive(Debug, Clone)]
pub struct NewDiffHunk {
    pub diff_hunk_id: String,
    pub schema_version: String,
    pub session_id: String,
    pub file_path: String,
    pub change_type: String,
    pub line_start_after: Option<i64>,
    pub line_end_after: Option<i64>,
    pub introduced_by_node_id: Option<String>,
    pub related_observed_event_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DiffHunkRow {
    pub diff_hunk_id: String,
    pub schema_version: String,
    pub session_id: String,
    pub file_path: String,
    pub change_type: String,
    pub line_start_after: Option<i64>,
    pub line_end_after: Option<i64>,
    pub introduced_by_node_id: Option<String>,
    pub related_observed_event_id: Option<String>,
}

pub async fn insert(pool: &SqlitePool, row: &NewDiffHunk) -> Result<()> {
    sqlx::query(
        "INSERT OR IGNORE INTO diff_hunk(
            diff_hunk_id, schema_version, session_id, file_path, change_type,
            line_start_after, line_end_after, introduced_by_node_id, related_observed_event_id)
         VALUES(?,?,?,?,?,?,?,?,?)",
    )
    .bind(&row.diff_hunk_id)
    .bind(&row.schema_version)
    .bind(&row.session_id)
    .bind(&row.file_path)
    .bind(&row.change_type)
    .bind(row.line_start_after)
    .bind(row.line_end_after)
    .bind(&row.introduced_by_node_id)
    .bind(&row.related_observed_event_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_session(pool: &SqlitePool, session_id: &str) -> Result<Vec<DiffHunkRow>> {
    let rows = sqlx::query(
        "SELECT diff_hunk_id, schema_version, session_id, file_path, change_type,
                line_start_after, line_end_after, introduced_by_node_id, related_observed_event_id
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
            line_start_after: r.get::<Option<i64>, _>("line_start_after"),
            line_end_after: r.get::<Option<i64>, _>("line_end_after"),
            introduced_by_node_id: r.get::<Option<String>, _>("introduced_by_node_id"),
            related_observed_event_id: r.get::<Option<String>, _>("related_observed_event_id"),
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

    #[tokio::test]
    async fn insert_then_list_session() {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        migrate(&pool).await.unwrap();
        let row = NewDiffHunk {
            diff_hunk_id: "hunk_1".into(),
            schema_version: "0.4.0".into(),
            session_id: "filesystem".into(),
            file_path: "a.rs".into(),
            change_type: "modified".into(),
            line_start_after: Some(42),
            line_end_after: Some(57),
            introduced_by_node_id: Some("nd_g_1".into()),
            related_observed_event_id: Some("ev_h_1".into()),
        };
        insert(&pool, &row).await.unwrap();
        let out = list_session(&pool, "filesystem").await.unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].diff_hunk_id, "hunk_1");
        assert_eq!(out[0].file_path, "a.rs");
        assert_eq!(out[0].line_start_after, Some(42));
        assert_eq!(count_by_session(&pool, "filesystem").await.unwrap(), 1);
    }

    #[tokio::test]
    async fn insert_or_ignore_dedup() {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        migrate(&pool).await.unwrap();
        let row = NewDiffHunk {
            diff_hunk_id: "hunk_2".into(),
            schema_version: "0.4.0".into(),
            session_id: "filesystem".into(),
            file_path: "b.rs".into(),
            change_type: "added".into(),
            line_start_after: None,
            line_end_after: None,
            introduced_by_node_id: None,
            related_observed_event_id: None,
        };
        insert(&pool, &row).await.unwrap();
        insert(&pool, &row).await.unwrap();
        assert_eq!(count_by_session(&pool, "filesystem").await.unwrap(), 1);
    }
}
