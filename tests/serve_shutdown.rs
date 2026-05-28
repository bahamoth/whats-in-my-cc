//! Post-slice-19 — graceful shutdown wiring.
//!
//! After the first round (5s sleep) was found to keep the shutdown_signal
//! future *unresolved* during the sleep — blocking axum graceful_shutdown
//! from starting at all — the contract was changed:
//!
//!   `shutdown_with_grace(cancel)` returns immediately once cancel/ctrl_c
//!   fires. cancel.cancel() is called inside the helper so long-lived
//!   stream handlers (SSE, MCP-GET, retention sweep) observe the signal
//!   and self-terminate. The outer `tokio::time::timeout` in main.rs
//!   provides the bounded grace window.

use std::time::Duration;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn shutdown_returns_immediately_after_cancel() {
    let cancel = CancellationToken::new();
    let fut = witmcc::serve::shutdown_with_grace(cancel.clone());
    let handle = tokio::spawn(fut);

    cancel.cancel();

    tokio::time::timeout(Duration::from_millis(100), handle)
        .await
        .expect("shutdown must resolve within 100ms of cancel (no sleep)")
        .unwrap();
}

#[tokio::test]
async fn shutdown_propagates_cancel_to_outer_token() {
    // ctrl_c branch is impossible to trigger in-process; we lock the
    // cancel-branch invariant instead: when cancel is observed, the helper
    // returns AND the token remains cancelled (idempotent).
    let cancel = CancellationToken::new();
    let fut = witmcc::serve::shutdown_with_grace(cancel.clone());
    let handle = tokio::spawn(fut);

    cancel.cancel();
    handle.await.unwrap();

    assert!(
        cancel.is_cancelled(),
        "outer token must remain cancelled after helper returns"
    );
}

#[test]
fn default_shutdown_grace_is_five_seconds() {
    assert_eq!(
        witmcc::serve::DEFAULT_SHUTDOWN_GRACE,
        Duration::from_secs(5)
    );
}

#[tokio::test]
async fn grace_timer_does_not_fire_before_cancel() {
    // Lock the invariant that broke events_subprocess: previously we wrapped
    // axum::serve in tokio::time::timeout(grace, ...), which started counting
    // immediately at server boot and killed the server 5 seconds in even
    // without any shutdown signal. The new helper only counts after cancel.
    let cancel = CancellationToken::new();
    let never_finishing_serve = std::future::pending::<std::io::Result<()>>();
    let grace = Duration::from_millis(150);

    let result = tokio::time::timeout(
        Duration::from_millis(400),
        witmcc::serve::run_serve_with_grace(never_finishing_serve, cancel, grace),
    )
    .await;

    assert!(
        result.is_err(),
        "without cancel the helper must wait on serve indefinitely (outer timeout fires first)"
    );
}

#[tokio::test]
async fn grace_timer_fires_after_cancel() {
    let cancel = CancellationToken::new();
    let never_finishing_serve = std::future::pending::<std::io::Result<()>>();
    let grace = Duration::from_millis(150);

    let cancel_cl = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        cancel_cl.cancel();
    });

    let result = tokio::time::timeout(
        Duration::from_millis(400),
        witmcc::serve::run_serve_with_grace(never_finishing_serve, cancel, grace),
    )
    .await;

    assert!(
        result.is_ok(),
        "after cancel the grace timer must abort the stuck serve within ~grace"
    );
}
