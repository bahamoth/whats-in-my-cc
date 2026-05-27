//! Slice-19 — Bearer token authentication middleware.
//!
//! Rejects requests without a valid `Authorization: Bearer <token>` header
//! with `401 Unauthorized`. Uses constant-time comparison via `subtle` to
//! avoid timing side-channels.

use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::Response,
};
use subtle::ConstantTimeEq;

use crate::api::AppState;

/// Middleware that requires a valid `Authorization: Bearer <token>` header.
///
/// Wired with `axum_mw::from_fn_with_state(state, require_token)`.
/// When `state.token` is empty (legacy test mode), all requests are passed through.
pub async fn require_token(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // If token is empty (test-only default), skip auth.
    if state.token.is_empty() {
        return Ok(next.run(req).await);
    }

    let header_val = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());

    let authorized = match header_val {
        Some(s) if s.starts_with("Bearer ") => {
            let provided = s["Bearer ".len()..].as_bytes();
            let expected = state.token.as_bytes();
            // Constant-time comparison (subtle::ConstantTimeEq).
            let same_len = provided.len() == expected.len();
            // subtle::ConstantTimeEq returns subtle::Choice; use into_bool().
            let bytes_match = bool::from(provided.ct_eq(expected));
            same_len && bytes_match
        }
        _ => false,
    };

    if authorized {
        Ok(next.run(req).await)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}
