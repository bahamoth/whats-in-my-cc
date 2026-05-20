//! Background git poller (slice-5, Task 9).
//!
//! Stub placeholder — replaced by the `git2`-driven poller in Task 9.

use std::path::PathBuf;
use tokio_util::sync::CancellationToken;

pub async fn run_git_poller(
    _pool: sqlx::SqlitePool,
    _repo_path: PathBuf,
    _interval_secs: u64,
    cancel: CancellationToken,
) -> anyhow::Result<()> {
    cancel.cancelled().await;
    Ok(())
}
