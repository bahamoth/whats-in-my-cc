//! Post-slice-19 — graceful shutdown wiring.
//!
//! `axum::serve(...).with_graceful_shutdown(fut)` defers shutdown until
//! `fut` resolves; only then does axum close the listener and wait for
//! in-flight connections to finish.
//!
//! Contract:
//!
//! * `shutdown_with_grace(cancel)` resolves the *moment* cancel or ctrl_c
//!   fires. It calls `cancel.cancel()` inside so long-lived stream
//!   handlers (SSE, MCP-GET, retention sweep) that subscribed to the
//!   same token can self-terminate.
//! * `run_serve_with_grace(serve_fut, cancel, grace)` races the serve
//!   future against a grace timer that **only starts counting after
//!   cancel fires**. This prevents the timer from killing a still-active
//!   server before any shutdown signal was sent.

use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub const DEFAULT_SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

pub async fn shutdown_with_grace(cancel: CancellationToken) {
    tokio::select! {
        _ = cancel.cancelled() => {}
        _ = tokio::signal::ctrl_c() => {
            cancel.cancel();
        }
    }
    tracing::info!("shutdown signal observed; closing listener");
}

/// Race `serve_fut` (typically `axum::serve(...).with_graceful_shutdown(...)`)
/// against a grace timer that **only begins after `cancel` is triggered**.
///
/// Behaviour:
///   * Neither cancel nor ctrl_c fired: waits indefinitely for `serve_fut`.
///   * Cancel/ctrl_c fired: axum drains in-flight handlers; if drain
///     completes within `grace`, `serve_fut` resolves normally.
///     Otherwise the timer wins, this function returns, and the dropped
///     `serve_fut` tears down whatever was stuck.
pub async fn run_serve_with_grace<F>(serve_fut: F, cancel: CancellationToken, grace: Duration)
where
    F: std::future::IntoFuture<Output = std::io::Result<()>>,
{
    let serve_fut = serve_fut.into_future();
    tokio::pin!(serve_fut);
    let grace_after_cancel = async {
        cancel.cancelled().await;
        tokio::time::sleep(grace).await;
    };
    tokio::pin!(grace_after_cancel);
    tokio::select! {
        r = &mut serve_fut => {
            if let Err(e) = r {
                tracing::error!(error = ?e, "axum::serve exited with error");
            }
        }
        _ = &mut grace_after_cancel => {
            tracing::warn!(
                grace_ms = grace.as_millis() as u64,
                "graceful shutdown exceeded grace window after cancel; aborting in-flight connections"
            );
        }
    }
}
