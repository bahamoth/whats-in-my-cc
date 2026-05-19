use axum::{
    body::Body,
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "$CARGO_MANIFEST_DIR/webui/dist"]
struct Assets;

pub async fn spa_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if !path.is_empty() {
        if let Some(content) = Assets::get(path) {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            return Response::builder()
                .header(header::CONTENT_TYPE, mime.as_ref())
                .body(Body::from(content.data.into_owned()))
                .unwrap();
        }
    }
    match Assets::get("index.html") {
        Some(content) => Response::builder()
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .body(Body::from(content.data.into_owned()))
            .unwrap(),
        None => (StatusCode::NOT_FOUND, "index.html missing from embed").into_response(),
    }
}
