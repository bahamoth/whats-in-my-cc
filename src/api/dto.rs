use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Serialize)]
pub struct OtelIngestResponse {
    pub accepted_spans: u64,
    pub rejected_spans: u64,
    pub duplicate_spans: u64,
    pub sessions_touched: Vec<String>,
}

/// slice-6 — receiver response for `/otel/v1/metrics`. Stage 1 fields plus the
/// Stage 2 per-data-point counters once the normaliser ran.
#[derive(Debug, Serialize)]
pub struct OtelMetricsRawResponse {
    pub accepted_resource_metrics: u64,
    pub stored_raw_rows: u64,
    pub duplicate_raw_rows: u64,
    pub accepted_data_points: u64,
    pub duplicate_data_points: u64,
    pub rejected_data_points: u64,
    pub sessions_touched: Vec<String>,
}

/// slice-6 — receiver response for `/otel/v1/logs`.
#[derive(Debug, Serialize)]
pub struct OtelLogsRawResponse {
    pub accepted_resource_logs: u64,
    pub stored_raw_rows: u64,
    pub duplicate_raw_rows: u64,
    pub accepted_log_records: u64,
    pub duplicate_log_records: u64,
    pub rejected_log_records: u64,
    pub sessions_touched: Vec<String>,
}

/// slice-6 — per-source freshness for `/v1/health/sources`. Powers `witmcc doctor`.
#[derive(Debug, Serialize)]
pub struct SourceHealth {
    pub label: String,
    pub last_ingested_at: Option<String>,
    pub row_count_24h: i64,
    pub total_rows: i64,
}

#[derive(Debug, Serialize)]
pub struct HealthSourcesResponse {
    pub sources: Vec<SourceHealth>,
}

#[derive(Debug, Serialize)]
pub struct HookIngestResponse {
    pub accepted_events: u64,
    pub rejected_events: u64,
    pub duplicate_events: u64,
    pub sessions_touched: Vec<String>,
}

#[derive(Serialize)]
pub struct SessionListItem {
    pub session_id: String,
    pub first_observed_at: String,
    pub last_observed_at: String,
    pub event_count: i64,
    pub source_uris: Vec<String>, // slice-1: empty array (tracked at observed_event payload level)
    /// slice-7 — per-kind row counts so the WebUI can surface
    /// transcript-only vs OTel-only sessions in the list view.
    #[serde(default)]
    pub by_kind: std::collections::BTreeMap<String, i64>,
}

#[derive(Serialize)]
pub struct SessionDetail {
    pub session_id: String,
    pub summary: SessionSummary,
    pub events: Vec<Value>,
}

#[derive(Serialize)]
pub struct SessionSummary {
    pub event_count: i64,
    pub by_kind: std::collections::BTreeMap<String, i64>,
    pub first_observed_at: String,
    pub last_observed_at: String,
}

#[derive(Serialize)]
pub struct GraphPayload {
    pub nodes: Vec<Value>,
    pub edges: Vec<Value>,
}

#[derive(Serialize)]
pub struct RawSource {
    pub kind: String,
    pub file_path: String,
    pub line_no: i64,
    pub ingested_at: String,
}

#[derive(Serialize)]
pub struct RawEventResponse {
    pub schema_version: String,
    pub event_id: String,
    pub session_id: String,
    pub source: RawSource,
    pub record: serde_json::Value,
    pub record_type: String,
    pub redaction_state: String,
    pub telemetry: Option<serde_json::Value>,
}
