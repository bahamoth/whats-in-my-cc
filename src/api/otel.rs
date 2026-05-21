use axum::{extract::State, http::StatusCode, Json};
use serde_json::json;

use crate::api::dto::{OtelIngestResponse, OtelLogsRawResponse, OtelMetricsRawResponse};
use crate::ingest::{otel, otel_logs, otel_metrics};
use crate::live::BroadcastSink;
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
    State(state): State<crate::api::AppState>,
    body: axum::body::Bytes,
) -> Result<Json<Envelope<OtelIngestResponse>>, (StatusCode, Json<serde_json::Value>)> {
    if body.len() > MAX_DECOMPRESSED_BYTES {
        return Err(payload_too_large());
    }
    let value: serde_json::Value = serde_json::from_slice(&body).map_err(bad_json)?;
    let parsed = otel::parse_otlp_json(&value);
    let sink = BroadcastSink::new(state.live_tx.clone());
    let result = otel::store(&state.pool, parsed, chrono::Utc::now(), &sink)
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

/// slice-6 — accept OTLP/JSON metrics request, persist raw (Stage 1), then
/// normalise into per-data-point MetricSample ObservedEvents (Stage 2).
pub async fn ingest_metrics(
    State(state): State<crate::api::AppState>,
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
    let received_at = chrono::Utc::now();
    let (inserted, raw_id, _sha) = otel::store_raw(
        &state.pool,
        SOURCE_TYPE_OTEL_METRICS,
        "otel-metrics",
        &value,
        received_at,
    )
    .await
    .map_err(db_failure)?;
    let (stored_raw_rows, duplicate_raw_rows) = if inserted { (1, 0) } else { (0, 1) };
    // Stage 2 is idempotent via insert_or_ignore so we can always run it. Re-POSTs
    // of an already-seen body will return all-duplicate_data_points and zero accepts.
    let sink = BroadcastSink::new(state.live_tx.clone());
    let stage2 = otel_metrics::store_request(&state.pool, &raw_id, &value, received_at, &sink)
        .await
        .map_err(db_failure)?;
    Ok(Json(Envelope {
        meta: ResponseMeta::now(),
        data: OtelMetricsRawResponse {
            accepted_resource_metrics,
            stored_raw_rows,
            duplicate_raw_rows,
            accepted_data_points: stage2.accepted_data_points,
            duplicate_data_points: stage2.duplicate_data_points,
            rejected_data_points: stage2.rejected_data_points,
            sessions_touched: stage2.sessions_touched,
        },
    }))
}

/// slice-6 — accept OTLP/JSON logs request, persist raw (Stage 1), then
/// normalise into per-record LogRecord ObservedEvents (Stage 2).
pub async fn ingest_logs(
    State(state): State<crate::api::AppState>,
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
    let received_at = chrono::Utc::now();
    let (inserted, raw_id, _sha) = otel::store_raw(
        &state.pool,
        SOURCE_TYPE_OTEL_LOGS,
        "otel-logs",
        &value,
        received_at,
    )
    .await
    .map_err(db_failure)?;
    let (stored_raw_rows, duplicate_raw_rows) = if inserted { (1, 0) } else { (0, 1) };
    let sink = BroadcastSink::new(state.live_tx.clone());
    let stage2 = otel_logs::store_request(&state.pool, &raw_id, &value, received_at, &sink)
        .await
        .map_err(db_failure)?;
    Ok(Json(Envelope {
        meta: ResponseMeta::now(),
        data: OtelLogsRawResponse {
            accepted_resource_logs,
            stored_raw_rows,
            duplicate_raw_rows,
            accepted_log_records: stage2.accepted_log_records,
            duplicate_log_records: stage2.duplicate_log_records,
            rejected_log_records: stage2.rejected_log_records,
            sessions_touched: stage2.sessions_touched,
        },
    }))
}
