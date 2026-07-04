pub mod dto;
pub mod mcp;
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
use tokio_util::sync::CancellationToken;
use tower_http::decompression::RequestDecompressionLayer;

use crate::api::mcp::SessionRegistry;
use crate::live::LiveEvent;

const MAX_REQUEST_BODY: usize = 4 * 1024 * 1024;

/// Slice-8 — shared state for axum handlers, including the in-process broadcast
/// channel that ingest writers emit into and the SSE handler subscribes to.
///
/// Slice-17 adds `mcp_sessions` — the in-memory MCP session registry.
///
/// Slice-19 adds `token` — bearer token for auth-gated endpoints.
/// Also adds `retention_profile` — the active retention profile name.
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
    /// Slice-17: MCP session registry (in-memory, DEV-S17-04).
    pub mcp_sessions: SessionRegistry,
    /// Slice-19: bearer token for API authentication.
    /// Empty string = auth disabled (only in legacy tests that haven't migrated).
    pub token: String,
    /// Slice-19: active retention profile name ("none" | "default" | "strict").
    pub retention_profile: String,
    /// Post-slice-19: cancellation token observed by long-lived stream
    /// handlers (SSE, MCP-GET) so they self-terminate on shutdown signal.
    pub shutdown: CancellationToken,
}

impl AppState {
    /// Test-only constructor. Builds a fresh broadcast channel with default
    /// capacity. `live_tx::receiver_count()` will be 0 at first; `BroadcastSink`
    /// tolerates that. MCP sessions start empty.
    /// Token defaults to empty string (auth middleware accepts any request when
    /// token is empty — callers that test auth must set `state.token`).
    pub fn new_for_tests(pool: SqlitePool) -> Self {
        let (tx, _) = broadcast::channel(512);
        Self {
            pool,
            live_tx: Arc::new(tx),
            sse_keepalive_secs: 30,
            sse_channel_capacity: 512,
            mcp_sessions: SessionRegistry::new(),
            token: String::new(),
            retention_profile: "none".to_string(),
            shutdown: CancellationToken::new(),
        }
    }
}

impl FromRef<AppState> for SqlitePool {
    fn from_ref(state: &AppState) -> Self {
        state.pool.clone()
    }
}

pub fn router(state: AppState) -> Router {
    // Routes that require bearer-token authentication (all /v1/* and /mcp).
    // Non-auth routes: /otel/* collectors, /v1/stream (SSE), static SPA.
    // (/hooks/*는 2026-06 hook collector 폐지로 존재하지 않는다.)
    // The AppState is added as a request extension so the auth middleware
    // can extract the expected token without a separate axum::extract::State call.
    let auth_state = state.clone();
    let authed = Router::new()
        .route("/mcp", post(mcp::transport::post_handler))
        .route("/mcp", get(mcp::transport::get_handler))
        .route("/v1/health", get(routes::health))
        .route("/v1/health/sources", get(routes::health_sources))
        .route("/v1/sessions", get(routes::list_sessions))
        .route("/v1/sessions/:id", get(routes::session_detail))
        .route("/v1/sessions/:id/events", get(routes::session_events))
        .route("/v1/sessions/:id/turns", get(routes::session_turns))
        .route("/v1/sessions/:id/tasks", get(routes::session_tasks))
        .route(
            "/v1/sessions/:id/diff-hunks",
            get(routes::session_diff_hunks),
        )
        .route(
            "/v1/sessions/:id/verification-runs",
            get(routes::session_verification_runs),
        )
        .route("/v1/sessions/:id/usage", get(routes::session_usage))
        .route("/v1/usage/baseline", get(routes::usage_baseline))
        .route("/v1/metrics", get(routes::metrics_series))
        .route(
            "/v1/verification/summary",
            get(routes::verification_summary),
        )
        .route("/v1/instructions/:sha", get(routes::instruction_snapshot))
        .route(
            "/v1/sessions/:id/instructions",
            get(routes::session_instructions),
        )
        .route(
            "/v1/verification-runs/:id",
            get(routes::verification_run_detail),
        )
        .route("/v1/sessions/:id/metrics", get(routes::session_metrics))
        .route(
            "/v1/sessions/:id/fingerprint",
            get(routes::session_fingerprint),
        )
        .route("/v1/sessions/:id/signals", get(routes::session_signals))
        .route("/v1/signals/:id", get(routes::signal_detail))
        .route("/v1/events/:event_id/raw", get(routes::event_raw))
        .route("/v1/audit", get(routes::list_audit))
        // B-4 (2026-07-04) — read-only 원칙의 유일한 write 예외: owner-only
        // local export. 데이터는 로컬 파일로만 쓰인다(외부 전송 없음).
        .route("/v1/export-bundles", post(routes::create_export_bundle))
        .route("/v1/detectors", get(routes::list_detectors))
        .route("/v1/plugins", get(routes::list_plugins))
        .layer(axum_mw::from_fn_with_state(
            auth_state,
            middleware::auth::require_token,
        ));

    Router::new()
        .merge(authed)
        .route("/otel/v1/traces", post(otel::ingest_traces))
        .route("/otel/v1/metrics", post(otel::ingest_metrics))
        .route("/otel/v1/logs", post(otel::ingest_logs))
        .route("/v1/stream", get(sse::stream_handler))
        .fallback(static_assets::spa_handler)
        .layer(DefaultBodyLimit::max(MAX_REQUEST_BODY))
        .layer(RequestDecompressionLayer::new().gzip(true))
        .layer(axum_mw::from_fn(middleware::host_allowlist))
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state)
}
