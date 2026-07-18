pub mod repo_audit;
pub mod repo_diff_hunk;
pub mod repo_observed;
pub mod repo_raw;
pub mod repo_retention;
pub mod repo_runs;
pub mod repo_signal;
pub mod repo_usage_facet;
pub mod repo_verification_run;

use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use std::str::FromStr;

use crate::error::Result;

pub async fn connect(url: &str) -> Result<SqlitePool> {
    let opts = sqlx::sqlite::SqliteConnectOptions::from_str(url)?
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(std::time::Duration::from_millis(5000))
        // perf-2026-06-29 — cap the WAL file at 64MiB. PASSIVE autocheckpoint
        // empties WAL frames into the main DB but never shrinks the file, so
        // without this the WAL keeps its historical peak (dogfood DB held 532MB
        // while only ~2MB was live). With the limit set, SQLite truncates the
        // WAL back to ≤64MiB after each checkpoint. This is disk hygiene, not a
        // read-latency fix (reads hit the wal-index hash, not the file length).
        .pragma("journal_size_limit", "67108864")
        // growth-2026-07-18 — the main DB file could only ever grow: deletes
        // (retention sweep) put pages on the freelist but nothing released
        // them. INCREMENTAL auto_vacuum takes effect on newly created DBs
        // only (the header of an existing DB wins until a manual VACUUM);
        // the sweep runs `PRAGMA incremental_vacuum` after each pass.
        .auto_vacuum(sqlx::sqlite::SqliteAutoVacuum::Incremental)
        .create_if_missing(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(opts)
        .await?;
    Ok(pool)
}

pub async fn migrate(pool: &SqlitePool) -> Result<()> {
    sqlx::migrate!("./migrations").run(pool).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// perf-2026-06-29 — PASSIVE autocheckpoint empties WAL frames but never
    /// truncates the file, so without a journal_size_limit the WAL keeps its
    /// historical peak size (the dogfood DB sat at 532MB while only ~2MB was
    /// live). Cap it at 64MiB so checkpointed WAL is truncated back down.
    #[tokio::test]
    async fn connect_caps_journal_size_limit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wal_cap.sqlite");
        let url = format!("sqlite://{}?mode=rwc", path.display());
        let pool = connect(&url).await.unwrap();
        let limit: i64 = sqlx::query_scalar("PRAGMA journal_size_limit")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            limit, 67_108_864,
            "connect() must cap journal_size_limit at 64MiB so a checkpointed \
             WAL is truncated instead of growing unbounded"
        );
    }
}
