//! Slice-17 — GET /mcp SSE channel test.
//!
//! Uses the tower oneshot + frame-reading approach (same as sse_integration.rs)
//! because axum-test fully-buffers responses and would hang on an open SSE stream.
//!
//! Tests:
//! - GET /mcp with a valid Mcp-Session-Id returns Content-Type: text/event-stream
//! - GET /mcp with no/unknown session returns 400/404
//! - The first SSE event contains the notifications/initialized method

use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::json;
use sqlx::sqlite::SqlitePoolOptions;
use tower::ServiceExt;
use wimcc::db::migrate;

const FRAME_TIMEOUT: Duration = Duration::from_secs(3);

async fn make_app() -> (axum::Router, sqlx::SqlitePool) {
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    let state = wimcc::api::AppState::new_for_tests(pool.clone());
    let app = wimcc::api::router(state);
    (app, pool)
}

/// POST initialize and return the Mcp-Session-Id.
async fn do_initialize(app: &axum::Router) -> String {
    let body_bytes = serde_json::to_vec(&json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "t", "version": "0"}
        }
    }))
    .unwrap();

    let req = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header("content-type", "application/json")
        .header("host", "127.0.0.1")
        .body(Body::from(body_bytes))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    resp.headers()
        .get("mcp-session-id")
        .expect("Mcp-Session-Id must be present after initialize")
        .to_str()
        .unwrap()
        .to_string()
}

/// Read SSE body frames until `expected_substring` appears, with a timeout.
/// Returns the accumulated body so far on success; panics on timeout.
async fn read_sse_until(body: axum::body::Body, expected: &str) -> String {
    let mut collected = String::new();
    let mut body = body;
    loop {
        let frame = tokio::time::timeout(FRAME_TIMEOUT, body.frame()).await;
        match frame {
            Err(_) => panic!(
                "timeout after {} s waiting for SSE substring {:?}. Got so far: {:?}",
                FRAME_TIMEOUT.as_secs(),
                expected,
                collected
            ),
            Ok(None) => panic!(
                "SSE stream closed before seeing {expected:?}. Got: {collected:?}"
            ),
            Ok(Some(Err(e))) => panic!("SSE body error: {e}"),
            Ok(Some(Ok(frame))) => {
                if let Ok(data) = frame.into_data() {
                    if let Ok(s) = std::str::from_utf8(&data) {
                        collected.push_str(s);
                        if collected.contains(expected) {
                            return collected;
                        }
                    }
                }
            }
        }
    }
}

#[tokio::test]
async fn mcp_sse_returns_text_event_stream_content_type() {
    let (app, _pool) = make_app().await;
    let sid = do_initialize(&app).await;

    let req = Request::builder()
        .method("GET")
        .uri("/mcp")
        .header("host", "127.0.0.1")
        .header("mcp-session-id", &sid)
        .header("accept", "text/event-stream")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let ct = resp
        .headers()
        .get("content-type")
        .expect("content-type must be present")
        .to_str()
        .unwrap();
    assert!(
        ct.contains("text/event-stream"),
        "content-type must be text/event-stream, got: {ct}"
    );
}

#[tokio::test]
async fn mcp_sse_emits_notifications_initialized_on_connect() {
    let (app, _pool) = make_app().await;
    let sid = do_initialize(&app).await;

    let req = Request::builder()
        .method("GET")
        .uri("/mcp")
        .header("host", "127.0.0.1")
        .header("mcp-session-id", &sid)
        .header("accept", "text/event-stream")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = resp.into_body();
    let got = read_sse_until(body, "notifications/initialized").await;
    assert!(
        got.contains("notifications/initialized"),
        "first SSE event must contain notifications/initialized. Got: {got:?}"
    );
}

#[tokio::test]
async fn mcp_sse_unknown_session_returns_404() {
    let (app, _pool) = make_app().await;

    let req = Request::builder()
        .method("GET")
        .uri("/mcp")
        .header("host", "127.0.0.1")
        .header("mcp-session-id", "mcps_nonexistent_session_id")
        .header("accept", "text/event-stream")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn mcp_sse_missing_session_returns_400() {
    let (app, _pool) = make_app().await;

    let req = Request::builder()
        .method("GET")
        .uri("/mcp")
        .header("host", "127.0.0.1")
        .header("accept", "text/event-stream")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}
