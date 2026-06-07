//! Slice-10a — locks the post-migration schema for `diff_hunk`. Asserts the
//! new column set is present and the legacy `introduced_by_commit_sha` /
//! slice-5 line-range column names are gone.

use sqlx::sqlite::SqlitePoolOptions;
use sqlx::Row;
use wimcc::db::migrate;

async fn fresh_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    pool
}

async fn columns(pool: &sqlx::SqlitePool, table: &str) -> Vec<String> {
    let rows = sqlx::query("SELECT name FROM pragma_table_info(?) ORDER BY cid")
        .bind(table)
        .fetch_all(pool)
        .await
        .unwrap();
    rows.into_iter()
        .map(|r| r.get::<String, _>("name"))
        .collect()
}

#[tokio::test]
async fn diff_hunk_schema_matches_slice10a_shape() {
    let pool = fresh_pool().await;
    let cols = columns(&pool, "diff_hunk").await;
    let must_have = [
        "diff_hunk_id",
        "schema_version",
        "session_id",
        "file_path",
        "change_type",
        "line_range_after_start",
        "line_range_after_end",
        "introduced_by_event_id",
        "introduced_by_tool_use_id",
        "patch_preview",
        "lines_added",
        "lines_removed",
        "user_modified",
        "created_at",
    ];
    for col in must_have {
        assert!(
            cols.iter().any(|c| c == col),
            "diff_hunk missing column `{col}`; columns: {cols:?}"
        );
    }
    // Legacy slice-5 columns must be absent.
    for legacy in ["introduced_by_commit_sha", "line_start_after", "line_end_after"] {
        assert!(
            !cols.iter().any(|c| c == legacy),
            "diff_hunk still has legacy column `{legacy}`; columns: {cols:?}"
        );
    }
    // slice-5 attribution column `introduced_by_node_id` is gone — attribution
    // is now event/tool-use only.
    for legacy in [
        "introduced_by_node_id",
        "related_observed_event_id",
    ] {
        assert!(
            !cols.iter().any(|c| c == legacy),
            "diff_hunk still has legacy attribution column `{legacy}`; columns: {cols:?}"
        );
    }
}

#[tokio::test]
async fn file_event_and_commit_tables_are_absent() {
    let pool = fresh_pool().await;
    for table in ["file_event", "commit"] {
        let row = sqlx::query("SELECT name FROM sqlite_master WHERE type='table' AND name=?")
            .bind(table)
            .fetch_optional(&pool)
            .await
            .unwrap();
        assert!(
            row.is_none(),
            "table `{table}` must not exist after slice-10a migrations"
        );
    }
}

/// Quantitative guard for the judge + graph layer removals (PR #37): the four
/// dropped tables must be absent from a freshly-migrated DB. Migrations 0018
/// (judge) and 0019 (graph) `DROP TABLE` these; reverting either — or a future
/// migration re-creating one — makes this fail. Pairs with #judge-removal /
/// #graph-removal in the implementation notes.
#[tokio::test]
async fn judge_and_graph_tables_are_absent() {
    let pool = fresh_pool().await;
    for table in [
        "judge_verdict_cache",
        "findings_pending_judge",
        "graph_node",
        "graph_edge",
    ] {
        let row = sqlx::query("SELECT name FROM sqlite_master WHERE type='table' AND name=?")
            .bind(table)
            .fetch_optional(&pool)
            .await
            .unwrap();
        assert!(
            row.is_none(),
            "table `{table}` must not exist after the judge/graph removal migrations (0018/0019)"
        );
    }
}

#[tokio::test]
async fn diff_hunk_indexes_present() {
    let pool = fresh_pool().await;
    let rows = sqlx::query(
        "SELECT name FROM sqlite_master WHERE type='index' AND tbl_name='diff_hunk'",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let names: Vec<String> = rows
        .into_iter()
        .map(|r| r.get::<String, _>("name"))
        .collect();
    for must in ["file_lineage_idx", "diff_hunk_event_idx", "diff_hunk_tool_use_idx"] {
        assert!(
            names.iter().any(|n| n == must),
            "diff_hunk missing index `{must}`; indexes: {names:?}"
        );
    }
}
