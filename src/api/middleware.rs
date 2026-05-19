use axum::{extract::Request, http::StatusCode, middleware::Next, response::Response};

fn is_allowed_host(bare: &str) -> bool {
    matches!(bare, "127.0.0.1" | "localhost")
}

pub async fn host_allowlist(req: Request, next: Next) -> Result<Response, StatusCode> {
    // Check explicit Host header first (standard HTTP/1.1 requests).
    if let Some(host) = req
        .headers()
        .get(axum::http::header::HOST)
        .and_then(|v| v.to_str().ok())
    {
        let bare = host.split(':').next().unwrap_or("");
        return if is_allowed_host(bare) {
            Ok(next.run(req).await)
        } else {
            Err(StatusCode::BAD_REQUEST)
        };
    }

    // Fall back to the URI authority (in-process / HTTP/2 requests where the
    // Host header is absent but the URI carries the authority).
    if let Some(authority) = req.uri().authority() {
        let bare = authority.host();
        return if is_allowed_host(bare) {
            Ok(next.run(req).await)
        } else {
            Err(StatusCode::BAD_REQUEST)
        };
    }

    // No Host header and no URI authority — reject (unexpected in practice).
    Err(StatusCode::BAD_REQUEST)
}
