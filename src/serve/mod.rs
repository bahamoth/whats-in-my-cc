//! Post-slice-19 — graceful shutdown helper with bounded grace window.
//!
//! `axum::serve(...).with_graceful_shutdown(fut)` waits for `fut` to resolve,
//! then waits for all in-flight connections to close. Long-lived SSE
//! subscribers (`/v1/stream`) keep their connection ESTABLISHED indefinitely,
//! so without a bounded grace window Ctrl+C appears to hang.
//!
//! `shutdown_with_grace` awaits cancel/ctrl_c first, then sleeps for `grace`
//! and returns — axum then drops the listener and the remaining connections
//! get closed by the runtime.

use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub const DEFAULT_SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

pub async fn shutdown_with_grace(cancel: CancellationToken, grace: Duration) {
    tokio::select! {
        _ = cancel.cancelled() => {}
        _ = tokio::signal::ctrl_c() => {}
    }
    tracing::info!(
        grace_ms = grace.as_millis() as u64,
        "shutdown signal received; closing listener within grace window"
    );
    tokio::time::sleep(grace).await;
}
