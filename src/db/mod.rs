pub mod repo_diff_hunk;
pub mod repo_graph;
pub mod repo_observed;
pub mod repo_raw;
pub mod repo_runs;
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
