//! Background filesystem watcher (slice-5, Task 8).
//!
//! Stub placeholder — replaced by the `notify`-driven loop in Task 8.

use std::path::PathBuf;
use tokio_util::sync::CancellationToken;

pub async fn run_file_watcher(
    _pool: sqlx::SqlitePool,
    _root: PathBuf,
    cancel: CancellationToken,
) -> anyhow::Result<()> {
    cancel.cancelled().await;
    Ok(())
}
