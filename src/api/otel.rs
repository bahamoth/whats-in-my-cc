use axum::{extract::State, http::StatusCode, Json};
use serde_json::json;
use sqlx::SqlitePool;

use crate::api::dto::{OtelIngestResponse, OtelLogsRawResponse, OtelMetricsRawResponse};
use crate::ingest::otel;
use crate::model::meta::{
    Envelope, ResponseMeta, SOURCE_TYPE_OTEL_LOGS, SOURCE_TYPE_OTEL_METRICS,
};

const MAX_DECOMPRESSED_BYTES: usize = 4 * 1024 * 1024;

fn payload_too_large() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::PAYLOAD_TOO_LARGE,
        Json(json!({
            "type": "about:blank",
            "title": "PAYLOAD_TOO_LARGE",
            "detail": format!("body exceeds {} bytes", MAX_DECOMPRESSED_BYTES),
        })),
    )
}

fn bad_json(err: serde_json::Error) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({
            "type": "about:blank",
            "title": "BAD_REQUEST",
            "detail": format!("json parse error: {err}"),
        })),
    )
}

fn db_failure(err: impl std::fmt::Display) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({
            "type": "about:blank",
            "title": "DB_FAILURE",
            "detail": format!("{err}"),
        })),
    )
}

pub async fn ingest_traces(
    State(pool): State<SqlitePool>,
    body: axum::body::Bytes,
) -> Result<Json<Envelope<OtelIngestResponse>>, (StatusCode, Json<serde_json::Value>)> {
    if body.len() > MAX_DECOMPRESSED_BYTES {
        return Err(payload_too_large());
    }
    let value: serde_json::Value = serde_json::from_slice(&body).map_err(bad_json)?;
    let parsed = otel::parse_otlp_json(&value);
    let result = otel::store(&pool, parsed, chrono::Utc::now())
        .await
        .map_err(db_failure)?;

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

/// slice-6 Stage 1 — accept OTLP/JSON metrics request, persist raw, no normalisation.
pub async fn ingest_metrics(
    State(pool): State<SqlitePool>,
    body: axum::body::Bytes,
) -> Result<Json<Envelope<OtelMetricsRawResponse>>, (StatusCode, Json<serde_json::Value>)> {
    if body.len() > MAX_DECOMPRESSED_BYTES {
        return Err(payload_too_large());
    }
    let value: serde_json::Value = serde_json::from_slice(&body).map_err(bad_json)?;
    let accepted_resource_metrics = value
        .get("resourceMetrics")
        .and_then(|v| v.as_array())
        .map(|a| a.len() as u64)
        .unwrap_or(0);
    let (inserted, _raw_id, _sha) = otel::store_raw(
        &pool,
        SOURCE_TYPE_OTEL_METRICS,
        "otel-metrics",
        &value,
        chrono::Utc::now(),
    )
    .await
    .map_err(db_failure)?;
    let (stored_raw_rows, duplicate_raw_rows) = if inserted { (1, 0) } else { (0, 1) };
    Ok(Json(Envelope {
        meta: ResponseMeta::now(),
        data: OtelMetricsRawResponse {
            accepted_resource_metrics,
            stored_raw_rows,
            duplicate_raw_rows,
        },
    }))
}

/// slice-6 Stage 1 — accept OTLP/JSON logs request, persist raw, no normalisation.
pub async fn ingest_logs(
    State(pool): State<SqlitePool>,
    body: axum::body::Bytes,
) -> Result<Json<Envelope<OtelLogsRawResponse>>, (StatusCode, Json<serde_json::Value>)> {
    if body.len() > MAX_DECOMPRESSED_BYTES {
        return Err(payload_too_large());
    }
    let value: serde_json::Value = serde_json::from_slice(&body).map_err(bad_json)?;
    let accepted_resource_logs = value
        .get("resourceLogs")
        .and_then(|v| v.as_array())
        .map(|a| a.len() as u64)
        .unwrap_or(0);
    let (inserted, _raw_id, _sha) = otel::store_raw(
        &pool,
        SOURCE_TYPE_OTEL_LOGS,
        "otel-logs",
        &value,
        chrono::Utc::now(),
    )
    .await
    .map_err(db_failure)?;
    let (stored_raw_rows, duplicate_raw_rows) = if inserted { (1, 0) } else { (0, 1) };
    Ok(Json(Envelope {
        meta: ResponseMeta::now(),
        data: OtelLogsRawResponse {
            accepted_resource_logs,
            stored_raw_rows,
            duplicate_raw_rows,
        },
    }))
}
