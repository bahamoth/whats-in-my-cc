use axum::{extract::State, http::StatusCode, Json};
use serde_json::json;
use sqlx::SqlitePool;

use crate::api::dto::OtelIngestResponse;
use crate::ingest::otel;
use crate::model::meta::{Envelope, ResponseMeta};

const MAX_DECOMPRESSED_BYTES: usize = 4 * 1024 * 1024;

pub async fn ingest_traces(
    State(pool): State<SqlitePool>,
    body: axum::body::Bytes,
) -> Result<Json<Envelope<OtelIngestResponse>>, (StatusCode, Json<serde_json::Value>)> {
    if body.len() > MAX_DECOMPRESSED_BYTES {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(json!({
                "type": "about:blank",
                "title": "PAYLOAD_TOO_LARGE",
                "detail": format!("body exceeds {} bytes", MAX_DECOMPRESSED_BYTES),
            })),
        ));
    }
    let value: serde_json::Value = serde_json::from_slice(&body).map_err(|err| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "type": "about:blank",
                "title": "BAD_REQUEST",
                "detail": format!("json parse error: {err}"),
            })),
        )
    })?;
    let parsed = otel::parse_otlp_json(&value);
    let result = otel::store(&pool, parsed, chrono::Utc::now())
        .await
        .map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "type": "about:blank",
                    "title": "DB_FAILURE",
                    "detail": format!("{err}"),
                })),
            )
        })?;

    Ok(Json(Envelope {
        meta: ResponseMeta::now(),
        data: OtelIngestResponse {
            accepted_spans: result.accepted_spans,
            rejected_spans: result.rejected_spans,
            duplicate_spans: result.duplicate_spans,
            sessions_touched: result.sessions_touched,
        },
    }))
}
