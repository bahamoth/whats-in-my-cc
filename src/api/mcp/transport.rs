//! Slice-17 — MCP Streamable HTTP transport handlers.
//!
//! POST /mcp — JSON-RPC over HTTP
//! GET  /mcp — SSE channel for server notifications
//!
//! Design ref: docs/superpowers/specs/2026-05-27-wimcc-slice17-mcp-streamable-http-design.md §4

use std::convert::Infallible;
use std::time::Duration;

use axum::{
    extract::State,
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Json,
    },
};
use futures::stream::{self, StreamExt};
use serde_json::{json, Value};
use tokio_stream::wrappers::BroadcastStream;

use crate::api::AppState;
use crate::api::mcp::{
    jsonrpc::{AnyResponse, Request as RpcRequest},
    methods::{handle, HandleResult},
};

/// MCP Origin allowlist: only localhost origins are permitted.
/// Absent Origin header (curl-style) is allowed (DEV-S17-07 context).
fn is_allowed_origin(origin: &str) -> bool {
    // Strip the scheme
    let bare = origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"))
        .unwrap_or(origin);
    // bare is now "127.0.0.1:PORT" or "localhost:PORT"
    let host = bare.split(':').next().unwrap_or("");
    matches!(host, "127.0.0.1" | "localhost")
}

/// POST /mcp — dispatch JSON-RPC request.
///
/// Reads `Mcp-Session-Id` from the request headers (required after initialize).
/// Returns the `Mcp-Session-Id` header on `initialize` responses.
pub async fn post_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    // Origin check: reject cross-origin requests.
    if let Some(origin) = headers
        .get("origin")
        .and_then(|v| v.to_str().ok())
    {
        if !is_allowed_origin(origin) {
            return (StatusCode::FORBIDDEN, HeaderMap::new(), axum::body::Bytes::new()).into_response();
        }
    }

    // Parse the JSON-RPC request.
    let req: RpcRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => {
            let err = AnyResponse::err(Value::Null, -32700, format!("Parse error: {e}"));
            let payload = serde_json::to_vec(&err).unwrap_or_default();
            return (
                StatusCode::OK,
                HeaderMap::new(),
                axum::body::Bytes::from(payload),
            ).into_response();
        }
    };

    // Extract Mcp-Session-Id from request headers.
    let req_session_id: Option<String> = headers
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    let result = handle(
        &req.method,
        req.id.clone(),
        &req.params,
        &state.pool,
        &state.mcp_sessions,
        req_session_id.as_deref(),
    )
    .await;

    match result {
        HandleResult::Silent | HandleResult::Initialized => {
            // Notification — 202 Accepted, no body.
            (StatusCode::ACCEPTED, HeaderMap::new(), axum::body::Bytes::new()).into_response()
        }
        HandleResult::Response(resp) => {
            // Extract the new session id if this was an initialize response.
            let new_session_id = if let AnyResponse::Ok(ref r) = resp {
                r.result["_sessionId"].as_str().map(str::to_string)
            } else {
                None
            };

            // Serialise (strip _sessionId from the wire payload).
            let wire_value = match &resp {
                AnyResponse::Ok(r) => {
                    // Remove internal _sessionId from the result before sending.
                    let mut v = r.result.clone();
                    if let Some(obj) = v.as_object_mut() {
                        obj.remove("_sessionId");
                    }
                    serde_json::to_vec(&json!({
                        "jsonrpc": "2.0",
                        "id": r.id,
                        "result": v
                    }))
                    .unwrap_or_default()
                }
                AnyResponse::Err(e) => serde_json::to_vec(e).unwrap_or_default(),
            };

            let mut resp_headers = HeaderMap::new();
            resp_headers.insert(
                axum::http::header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            );
            if let Some(sid) = new_session_id {
                if let Ok(hv) = HeaderValue::from_str(&sid) {
                    resp_headers.insert(
                        HeaderName::from_static("mcp-session-id"),
                        hv,
                    );
                }
            }

            (StatusCode::OK, resp_headers, axum::body::Bytes::from(wire_value)).into_response()
        }
    }
}

/// GET /mcp — SSE notification channel.
///
/// Requires `Mcp-Session-Id` header. Unknown session → 404.
/// Emits notifications/initialized on connect, then resources/updated on rebuild.
pub async fn get_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // Origin check.
    if let Some(origin) = headers.get("origin").and_then(|v| v.to_str().ok()) {
        if !is_allowed_origin(origin) {
            return StatusCode::FORBIDDEN.into_response();
        }
    }

    let session_id = headers
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    let session_id = match session_id {
        Some(s) => s,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "Mcp-Session-Id header required"})),
            )
                .into_response();
        }
    };

    // Check session exists.
    if !state.mcp_sessions.exists(&session_id).await {
        return StatusCode::NOT_FOUND.into_response();
    }

    // Subscribe to notifications.
    let rx = match state.mcp_sessions.subscribe(&session_id).await {
        Some(r) => r,
        None => return StatusCode::NOT_FOUND.into_response(),
    };

    // First frame: synthetic notifications/initialized to confirm channel is live.
    let init_event = Ok::<_, Infallible>(
        Event::default()
            .event("message")
            .data(serde_json::to_string(&json!({
                "jsonrpc": "2.0",
                "method": "notifications/initialized"
            }))
            .unwrap_or_default()),
    );
    let first = stream::once(async move { init_event });

    // Live broadcast stream.
    let live = BroadcastStream::new(rx).filter_map(|item| async move {
        match item {
            Ok(notif) => {
                let payload = serde_json::to_string(&json!({
                    "jsonrpc": "2.0",
                    "method": notif.method,
                    "params": notif.params
                }))
                .ok()?;
                Some(Ok::<_, Infallible>(Event::default().event("message").data(payload)))
            }
            Err(_) => None,
        }
    });

    let combined = first.chain(live);

    // Server-side cancellation on shutdown — same pattern as /v1/stream.
    let shutdown = state.shutdown.clone();
    let cancellable = combined.take_until(async move { shutdown.cancelled().await });

    Sse::new(cancellable)
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(30))
                .text("keepalive"),
        )
        .into_response()
}
