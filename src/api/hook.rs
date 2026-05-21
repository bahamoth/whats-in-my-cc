use axum::{extract::State, http::StatusCode, Json};
use serde_json::json;

use crate::api::dto::HookIngestResponse;
use crate::ingest::hook;
use crate::live::BroadcastSink;
use crate::model::meta::{Envelope, ResponseMeta};

const MAX_HOOK_BODY: usize = 1024 * 1024;

pub async fn ingest_events(
    State(state): State<crate::api::AppState>,
    body: axum::body::Bytes,
) -> Result<Json<Envelope<HookIngestResponse>>, (StatusCode, Json<serde_json::Value>)> {
    if body.len() > MAX_HOOK_BODY {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(json!({
                "type": "about:blank",
                "title": "PAYLOAD_TOO_LARGE",
                "detail": format!("body exceeds {} bytes", MAX_HOOK_BODY),
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
    let parsed = hook::parse_body(&value);
    let sink = BroadcastSink::new(state.live_tx.clone());
    let result = hook::store(&state.pool, parsed, chrono::Utc::now(), &sink)
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
        data: HookIngestResponse {
            accepted_events: result.accepted_events,
            rejected_events: result.rejected_events,
            duplicate_events: result.duplicate_events,
            sessions_touched: result.sessions_touched,
        },
    }))
}
