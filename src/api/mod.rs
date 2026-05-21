pub mod dto;
pub mod hook;
pub mod middleware;
pub mod otel;
pub mod routes;
pub mod static_assets;

use axum::{
    extract::DefaultBodyLimit, middleware as axum_mw, routing::get, routing::post, Router,
};
use sqlx::SqlitePool;
use tower_http::decompression::RequestDecompressionLayer;

const MAX_REQUEST_BODY: usize = 4 * 1024 * 1024;

pub fn router(pool: SqlitePool) -> Router {
    Router::new()
        .route("/v1/health", get(routes::health))
        .route("/v1/health/sources", get(routes::health_sources))
        .route("/v1/sessions", get(routes::list_sessions))
        .route("/v1/sessions/:id", get(routes::session_detail))
        .route("/v1/sessions/:id/graph", get(routes::session_graph))
        .route("/v1/events/:event_id/raw", get(routes::event_raw))
        .route("/otel/v1/traces", post(otel::ingest_traces))
        .route("/otel/v1/metrics", post(otel::ingest_metrics))
        .route("/otel/v1/logs", post(otel::ingest_logs))
        .route("/hooks/v1/events", post(hook::ingest_events))
        .fallback(static_assets::spa_handler)
        .layer(DefaultBodyLimit::max(MAX_REQUEST_BODY))
        .layer(RequestDecompressionLayer::new().gzip(true))
        .layer(axum_mw::from_fn(middleware::host_allowlist))
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(pool)
}
