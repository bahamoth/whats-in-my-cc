//! Post-slice-19 — graceful shutdown grace period.
//!
//! Locks the invariant that `serve::shutdown_with_grace`:
//!   1. Awaits cancel/ctrl_c.
//!   2. Then sleeps `grace` duration to give axum a bounded close window.
//!   3. Returns even if long-lived SSE connections are still open.
//!
//! Real-world bug: SSE consumers keep the connection ESTABLISHED, axum
//! waits indefinitely for them to drain, Ctrl+C appears to hang.

use std::time::Duration;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn shutdown_returns_after_cancel_plus_grace() {
    let cancel = CancellationToken::new();
    let grace = Duration::from_millis(150);

    let fut = witmcc::serve::shutdown_with_grace(cancel.clone(), grace);
    let handle = tokio::spawn(fut);

    cancel.cancel();

    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        !handle.is_finished(),
        "must not return before grace elapses (grace=150ms, waited 50ms)"
    );

    tokio::time::sleep(Duration::from_millis(250)).await;
    assert!(
        handle.is_finished(),
        "must return within grace + epsilon (grace=150ms, total wait=300ms)"
    );
    handle.await.unwrap();
}

#[test]
fn default_grace_is_five_seconds() {
    assert_eq!(
        witmcc::serve::DEFAULT_SHUTDOWN_GRACE,
        Duration::from_secs(5)
    );
}
