use serde::Serialize;
use serde_json::Value;

/// Plan 3a — wrapper for `GET /v1/sessions/:id/metrics` response body.
/// `data` carries `SessionMetrics` which is already `Serialize`.
#[derive(Serialize)]
pub struct SessionMetricsResponse {
    pub data: crate::insight::metrics::SessionMetrics,
}

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

/// slice-6 — per-source freshness for `/v1/health/sources`. Powers `wimcc doctor`.
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

/// Slice-9 — `events` field removed. Use `GET /v1/sessions/:id/events?...`
/// for cursor-paged event windows. Replaces slice-8's DEV-S8-14 newest-5000
/// cap workaround.
#[derive(Serialize)]
pub struct SessionDetail {
    pub session_id: String,
    pub summary: SessionSummary,
}

/// Slice-9 — response payload for `GET /v1/sessions/:id/events`.
/// Cursors are `<observed_at_rfc3339>|<event_id>` (see `model::cursor`).
/// `next_cursor: null` means the window already reaches the session's live
/// tip; further updates arrive via SSE rather than another page fetch.
#[derive(Serialize)]
pub struct SessionEventsResponse {
    pub events: Vec<Value>,
    pub prev_cursor: Option<String>,
    pub next_cursor: Option<String>,
}

#[derive(Serialize)]
pub struct SessionSummary {
    pub event_count: i64,
    pub by_kind: std::collections::BTreeMap<String, i64>,
    pub first_observed_at: String,
    pub last_observed_at: String,
}

/// Slice-10a follow-up — JSON shape returned by
/// `GET /v1/sessions/{id}/diff-hunks`. Fields mirror the SQLite `diff_hunk`
/// row 1:1 so reviewers can verify transcript-derived attribution from the
/// API alone.
#[derive(Serialize)]
pub struct DiffHunkDto {
    pub diff_hunk_id: String,
    pub session_id: String,
    pub file_path: String,
    pub change_type: String,
    pub line_range_after_start: Option<i64>,
    pub line_range_after_end: Option<i64>,
    pub introduced_by_event_id: String,
    pub introduced_by_tool_use_id: Option<String>,
    pub patch_preview: String,
    pub lines_added: i64,
    pub lines_removed: i64,
    pub user_modified: bool,
}

#[derive(Serialize)]
pub struct DiffHunksResponse {
    pub hunks: Vec<DiffHunkDto>,
}

/// Slice-11 — single verification run in the Pull API response.
/// `covered_diff_hunk_ids` is computed at response time from the temporal
/// precedence rule (DEV-S11-02: not stored as a column).
#[derive(Serialize)]
pub struct VerificationRunDto {
    pub verification_run_id: String,
    pub schema_version: String,
    pub session_id: String,
    pub source: String,
    pub command: String,
    pub command_kind: String,
    pub trigger_event_id: String,
    pub trigger_tool_use_id: Option<String>,
    pub status: String,
    pub status_provenance: Option<String>,
    pub detection_basis: String,
    pub status_basis: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub exit_code: Option<i32>,
    pub failure_summary: Option<String>,
    pub covered_diff_hunk_ids: Vec<String>,
}

#[derive(Serialize)]
pub struct VerificationRunsResponse {
    pub data: Vec<VerificationRunDto>,
}

#[derive(Serialize)]
pub struct VerificationRunDetailResponse {
    pub data: VerificationRunDto,
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

/// Plan 1 — a single Signal in the Pull API response. Signals are deterministic
/// facts: NO severity/confidence/status (those are judgments). `evidence_refs`,
/// `facts`, and `provenance` are parsed from JSON columns before serialisation
/// so callers receive typed values.
#[derive(Serialize)]
pub struct SignalDto {
    pub signal_id: String,
    pub schema_version: String,
    pub session_id: String,
    pub detector: String,
    pub subkind: Option<String>,
    pub summary: String,
    pub evidence_refs: Vec<serde_json::Value>,
    pub facts: serde_json::Value,
    pub provenance: serde_json::Value,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct SignalsResponse {
    pub data: Vec<SignalDto>,
}

/// insight-redesign #1 + #5(cost) — session token-usage aggregate, now with a
/// public-pricing **estimate** of dollar cost (Q2). `estimated_cost_usd` is
/// NOT actual billing: `cost_basis = "estimate_public_pricing"` and the UI
/// badges it 추정. Replaced by the OTel `claude_code.cost.usage` metric if/when
/// it arrives (spec §6.5).
#[derive(Serialize)]
pub struct SessionUsageDto {
    pub session_id: String,
    pub turns: i64,
    pub input_tokens: i64,
    pub cache_creation_input_tokens: i64,
    pub cache_read_input_tokens: i64,
    pub output_tokens: i64,
    /// input + cache_creation + output (cache_read is NOT billed)
    pub billed_tokens: i64,
    /// cache_read / (cache_read + cache_creation + input); null when denom 0
    pub cache_hit_ratio: Option<f64>,
    /// Estimated session cost in USD — public-pricing ESTIMATE, never actual
    /// billing. See `cost_basis` / `pricing_version`.
    pub estimated_cost_usd: f64,
    /// Always "estimate_public_pricing" for this slice — drives the 추정 badge.
    pub cost_basis: String,
    /// Rate-table version the estimate was computed against.
    pub pricing_version: String,
    /// Models in this session we could not price (excluded from the total);
    /// surfaced so the UI can disclose incomplete cost coverage.
    pub models_without_pricing: Vec<String>,
    pub by_model: Vec<ModelUsageDto>,
}

#[derive(Serialize)]
pub struct ModelUsageDto {
    pub model: String,
    pub turns: i64,
    pub input_tokens: i64,
    pub cache_creation_input_tokens: i64,
    pub cache_read_input_tokens: i64,
    pub output_tokens: i64,
    /// Per-model public-pricing ESTIMATE in USD (0 when the model is unpriced).
    pub estimated_cost_usd: f64,
    /// false when no pricing entry exists for `model` (cost is then 0).
    pub priced: bool,
}

/// insight-redesign #6 — one baseline metric's quantile triple.
/// `median` is the user's rolling norm; the frontend renders the measured
/// session value as a delta against it ("vs your median"). All three are null
/// when no session in the store has usage_facet rows for this metric.
#[derive(Serialize)]
pub struct BaselineStat {
    pub p25: Option<f64>,
    pub median: Option<f64>,
    pub p75: Option<f64>,
}

/// insight-redesign #6 — cross-session usage baseline. Median (+ p25/p75) of
/// each key metric across ALL stored sessions that have usage_facet rows.
/// `session_count` is the number of sessions the baseline was computed over.
#[derive(Serialize)]
pub struct UsageBaselineDto {
    pub session_count: i64,
    pub cache_hit_ratio: BaselineStat,
    pub billed_tokens: BaselineStat,
    pub turns: BaselineStat,
    pub output_tokens: BaselineStat,
}

