// File watcher integration tests. Gated behind `#[ignore]` because filesystem
// event timing is OS-/CI-dependent. Run locally with:
//   cargo test --test file_watcher_loop -- --ignored

use std::time::Duration;
use witmcc::db::{migrate, repo_observed};
use witmcc::ingest::file_git::FILESYSTEM_SESSION_ID;
use witmcc::watcher::run_file_watcher;

async fn fresh_pool() -> sqlx::SqlitePool {
    use sqlx::sqlite::SqlitePoolOptions;
    let pool = SqlitePoolOptions::new()
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    pool
}

#[tokio::test]
#[ignore = "FS event timing is OS-dependent; run locally with --ignored"]
async fn watcher_observes_file_creation_within_2s() {
    let dir = tempfile::tempdir().unwrap();
    let pool = fresh_pool().await;
    let cancel = tokio_util::sync::CancellationToken::new();
    let tok = cancel.clone();
    let pool_cl = pool.clone();
    let root = dir.path().to_path_buf();
    let (live_tx, _) = tokio::sync::broadcast::channel::<witmcc::live::LiveEvent>(64);
    let live_tx = std::sync::Arc::new(live_tx);
    let handle = tokio::spawn(async move {
        run_file_watcher(pool_cl, root, live_tx, tok).await.unwrap();
    });
    // Give the watcher a moment to register.
    tokio::time::sleep(Duration::from_millis(200)).await;
    let target = dir.path().join("a.txt");
    std::fs::write(&target, b"hi").unwrap();

    // Poll for up to 2 s.
    let mut found = 0;
    for _ in 0..40 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let rows = repo_observed::list_session(&pool, FILESYSTEM_SESSION_ID, 100)
            .await
            .unwrap();
        if !rows.is_empty() {
            found = rows.len();
            break;
        }
    }

    cancel.cancel();
    let _ = handle.await;

    assert!(found > 0, "expected ≥1 file_event row, got {found}");
}
