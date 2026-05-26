pub mod dto;
pub mod hook;
pub mod middleware;
pub mod otel;
pub mod routes;
pub mod sse;
pub mod static_assets;

use std::sync::Arc;

use axum::{
    extract::{DefaultBodyLimit, FromRef},
    middleware as axum_mw,
    routing::get,
    routing::post,
    Router,
};
use sqlx::SqlitePool;
use tokio::sync::broadcast;
use tower_http::decompression::RequestDecompressionLayer;

use crate::live::LiveEvent;

const MAX_REQUEST_BODY: usize = 4 * 1024 * 1024;

/// Slice-8 — shared state for axum handlers, including the in-process broadcast
/// channel that ingest writers emit into and the SSE handler subscribes to.
///
/// `FromRef<AppState> for SqlitePool` is provided so existing handlers that
/// declare `State<SqlitePool>` continue to compile without source change; only
/// handlers that actually need `live_tx` (currently the SSE handler) take
/// `State<AppState>` directly.
#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub live_tx: Arc<broadcast::Sender<LiveEvent>>,
    pub sse_keepalive_secs: u64,
    pub sse_channel_capacity: usize,
}

impl AppState {
    /// Test-only constructor. Builds a fresh broadcast channel with default
    /// capacity. `live_tx::receiver_count()` will be 0 at first; `BroadcastSink`
    /// tolerates that.
    pub fn new_for_tests(pool: SqlitePool) -> Self {
        let (tx, _) = broadcast::channel(512);
        Self {
            pool,
            live_tx: Arc::new(tx),
            sse_keepalive_secs: 30,
            sse_channel_capacity: 512,
        }
    }
}

impl FromRef<AppState> for SqlitePool {
    fn from_ref(state: &AppState) -> Self {
        state.pool.clone()
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/v1/health", get(routes::health))
        .route("/v1/health/sources", get(routes::health_sources))
        .route("/v1/sessions", get(routes::list_sessions))
        .route("/v1/sessions/:id", get(routes::session_detail))
        .route("/v1/sessions/:id/events", get(routes::session_events))
        .route("/v1/sessions/:id/graph", get(routes::session_graph))
        .route(
            "/v1/sessions/:id/diff-hunks",
            get(routes::session_diff_hunks),
        )
        .route("/v1/events/:event_id/raw", get(routes::event_raw))
        .route("/otel/v1/traces", post(otel::ingest_traces))
        .route("/otel/v1/metrics", post(otel::ingest_metrics))
        .route("/otel/v1/logs", post(otel::ingest_logs))
        .route("/hooks/v1/events", post(hook::ingest_events))
        .route("/v1/stream", get(sse::stream_handler))
        .fallback(static_assets::spa_handler)
        .layer(DefaultBodyLimit::max(MAX_REQUEST_BODY))
        .layer(RequestDecompressionLayer::new().gzip(true))
        .layer(axum_mw::from_fn(middleware::host_allowlist))
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state)
}
