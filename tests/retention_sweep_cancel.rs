//! Post-slice-19 — retention sweep cancel wiring.
//!
//! Previous spawn_sweep_task had no cancel handle: `loop { tick.await; sweep; }`
//! ran forever, so `bg_handles.join()` would hang on shutdown when the policy
//! was active. We thread CancellationToken so the task self-exits.

use std::time::Duration;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn sweep_task_exits_on_cancel() {
    let pool = wimcc::db::connect(":memory:").await.unwrap();
    wimcc::db::migrate(&pool).await.unwrap();

    let policy = wimcc::security::retention::RetentionPolicy {
        profile: wimcc::security::retention::Profile::Default,
    };
    let cancel = CancellationToken::new();

    let handle =
        wimcc::security::retention::spawn_sweep_task(pool, policy, cancel.clone());

    // Give the task a tick to install its select; then cancel.
    tokio::time::sleep(Duration::from_millis(50)).await;
    cancel.cancel();

    tokio::time::timeout(Duration::from_millis(500), handle)
        .await
        .expect("sweep task must exit within 500ms of cancel")
        .unwrap();
}

#[tokio::test]
async fn sweep_task_none_profile_exits_immediately_regardless_of_cancel() {
    let pool = wimcc::db::connect(":memory:").await.unwrap();
    wimcc::db::migrate(&pool).await.unwrap();

    let policy = wimcc::security::retention::RetentionPolicy {
        profile: wimcc::security::retention::Profile::None,
    };
    let cancel = CancellationToken::new();

    let handle =
        wimcc::security::retention::spawn_sweep_task(pool, policy, cancel.clone());

    tokio::time::timeout(Duration::from_millis(200), handle)
        .await
        .expect("none-profile task must exit immediately")
        .unwrap();
}
