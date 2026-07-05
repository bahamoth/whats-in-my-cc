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
    /// S6 (UX 재설계) — identifiability facets, all nullable. The WebUI shows
    /// the slug (falling back to the UUID), a project pill, a dominant-model
    /// tag, and a one-line first-message preview.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_user_message_preview: Option<String>,
    /// Teammate 세션 식별 (2026-07-03) — named Agent 스폰이 만드는 별도
    /// 최상위 세션의 envelope 필드. WebUI가 팀 그룹핑·리드 조인에 쓴다.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_name: Option<String>,
}

/// Slice-9 — `events` field removed. Use `GET /v1/sessions/:id/events?...`
/// for cursor-paged event windows. Replaces slice-8's DEV-S8-14 newest-5000
/// cap workaround.
#[derive(Serialize)]
pub struct SessionDetail {
    pub session_id: String,
    /// B-6c (2026-07-04) — teammate 세션의 agent 타입("Explore" 등).
    /// session_state/agent_setting 이벤트에서 live 집계(세션 상수, 표본 1).
    /// teammate가 아닌 세션은 null.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_setting: Option<String>,
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
    /// §1.2 — total events matching the active filter across the whole
    /// session (not just this window). `None` (omitted) when no filter axis
    /// is active — the field only exists when a filter narrowed the window.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_count: Option<i64>,
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
    /// doc-audit-2026-06-10 — mirrors `raw_event.redaction_state`
    /// ("redacted" | "not_redacted" | "not_applicable" per doc 05).
    /// JSON null for legacy rows ingested before the column existed.
    pub redaction_state: Option<String>,
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

/// insight-redesign #1 + #5(cost) + F1(rename) — session token-usage aggregate.
/// `estimated_cost_usd` is NOT actual billing: `cost_basis = "estimate_public_pricing"`.
/// Replaced by the OTel `claude_code.cost.usage` metric if/when it arrives (spec §6.5).
///
/// F1 changes:
/// - `turns` → `assistant_events`: usage_facet row count (assistant output events).
/// - `user_turns` added: distinct `turn_id` count across `observed_event` for this session.
/// - `cache_hit_ratio` removed: window-fixed rate; consumers compute from token components.
#[derive(Serialize)]
pub struct SessionUsageDto {
    pub session_id: String,
    /// Number of usage_facet rows for this session (= assistant output events).
    pub assistant_events: i64,
    /// Distinct turn_id count in observed_event for this session (= user prompts).
    pub user_turns: i64,
    pub input_tokens: i64,
    pub cache_creation_input_tokens: i64,
    pub cache_read_input_tokens: i64,
    pub output_tokens: i64,
    /// input + cache_creation + output (cache_read is NOT billed)
    pub billed_tokens: i64,
    /// Estimated session cost in USD — public-pricing ESTIMATE, never actual
    /// billing. See `cost_basis` / `pricing_version`.
    pub estimated_cost_usd: f64,
    /// Always "estimate_public_pricing" for this slice — drives the 추정 badge.
    pub cost_basis: String,
    /// Pricing-table provenance `pricing_estimate@<YYYY-MM-DD>` — the date is the
    /// last refresh of the rate table (no arbitrary v-numbering), so the UI can
    /// show/flag the estimate's age directly.
    pub pricing_version: String,
    /// Models in this session we could not price (excluded from the total);
    /// surfaced so the UI can disclose incomplete cost coverage.
    pub models_without_pricing: Vec<String>,
    pub by_model: Vec<ModelUsageDto>,
}

/// §2.2 (2026-07-04 세션 상세 개선) — 이 모델에 적용된 per-Mtoken USD 단가
/// (공개 가격표 ESTIMATE). 가격표에 없는 모델은 None(→ JSON null).
#[derive(Serialize)]
pub struct ModelRatesDto {
    pub input_per_mtok: f64,
    pub cache_creation_per_mtok: f64,
    pub cache_read_per_mtok: f64,
    pub output_per_mtok: f64,
}

#[derive(Serialize)]
pub struct ModelUsageDto {
    pub model: String,
    /// 모델별 usage_facet 행 수(=assistant_events). user_turns는 세션 레벨이라 per-model 없음.
    pub assistant_events: i64,
    pub input_tokens: i64,
    pub cache_creation_input_tokens: i64,
    pub cache_read_input_tokens: i64,
    pub output_tokens: i64,
    /// Per-model public-pricing ESTIMATE in USD (0 when the model is unpriced).
    pub estimated_cost_usd: f64,
    /// false when no pricing entry exists for `model` (cost is then 0).
    pub priced: bool,
    /// 적용 단가 4종 — 미가격 모델은 null. 가격표 SSOT는 백엔드(pricing.json).
    pub rates: Option<ModelRatesDto>,
}

/// insight-redesign #6 + PR-3 §3a — one baseline metric's quantile triple.
/// `median` is the user's rolling norm; the frontend renders the measured
/// session value as a delta against it ("vs your median"). All three are null
/// when no session in the scope has a value for this metric.
/// `n` = 이 지표의 분포에 실제로 들어간 세션 수(지표별 게이트가 달라 서로 다를
/// 수 있다 — cache_hit은 분모>0, pass_rate는 측정>0, tool_failure는 tool_call>0,
/// cost는 billed>0). n<3이면 프론트가 "표본 부족"으로 강조를 해제한다.
#[derive(Serialize)]
pub struct BaselineStat {
    pub p25: Option<f64>,
    pub median: Option<f64>,
    pub p75: Option<f64>,
    pub n: i64,
}

/// insight-redesign #6 + PR-3 §3a — cross-session usage baseline. Median
/// (+ p25/p75) of each key metric across the stored sessions in scope.
/// `session_count` is the number of usage sessions the baseline was computed
/// over. `?session_id=`가 오면 그 세션의 project(session_summary facet)로 분포를
/// 스코프한다. project 미상이면 store 전체로 폴백(scope="store").
///
/// F1: `turns` renamed to `assistant_events` (quantile distribution of
/// usage_facet row counts per session). `cache_hit_ratio` distribution retained
/// here — unlike the per-session scalar, the baseline exposes a *quantile
/// distribution* across sessions, which is a valid cross-session comparison.
#[derive(Serialize)]
pub struct UsageBaselineDto {
    pub session_count: i64,
    /// "project" | "store" — 프론트가 라벨을 정직하게 붙이기 위한 관측 사실.
    pub scope: String,
    pub project: Option<String>,
    pub cache_hit_ratio: BaselineStat,
    pub billed_tokens: BaselineStat,
    pub assistant_events: BaselineStat,
    pub output_tokens: BaselineStat,
    /// passed/(passed+failed) per session — 측정(passed+failed>0) 세션만.
    pub verification_pass_rate: BaselineStat,
    /// tool_failure 시그널 수 per session — tool_call_total>0 세션만(0-인플레 방지).
    pub tool_failure_count: BaselineStat,
    /// 공개 가격표 추정 비용 per session — billed_tokens>0 세션만.
    pub estimated_cost_usd: BaselineStat,
}

/// `GET /v1/sessions/:id/tasks` — per-task summary (TaskCreate/TaskUpdate
/// correlated + work-span window aggregations). See `task_summary` aggregator.
#[derive(Serialize)]
pub struct TaskTransitionDto {
    pub status: String,
    pub at_ms: i64,
    pub event_id: String,
}
#[derive(Serialize)]
pub struct TaskVerifDto {
    pub passed: u32,
    pub failed: u32,
    pub unknown: u32,
    pub not_executed: u32,
}
#[derive(Serialize)]
pub struct TaskTokensDto {
    pub input: i64,
    pub output: i64,
    pub cache_creation: i64,
    pub cache_read: i64,
}
#[derive(Serialize)]
pub struct TaskHistEntryDto {
    pub tag: String,
    pub count: u32,
}
#[derive(Serialize)]
pub struct TaskDto {
    pub task_id: String,
    pub subject: String,
    pub description: Option<String>,
    pub active_form: Option<String>,
    /// TaskCreate event_id — the glance row jumps the replay here.
    pub event_id: String,
    pub created_at_ms: i64,
    pub status: String,
    pub transitions: Vec<TaskTransitionDto>,
    pub duration_ms: Option<i64>,
    pub work_duration_ms: Option<i64>,
    pub saw_in_progress: bool,
    pub activity_count: Option<u32>,
    pub tag_histogram: Vec<TaskHistEntryDto>,
    pub lines_added: Option<i64>,
    pub lines_removed: Option<i64>,
    pub verification: Option<TaskVerifDto>,
    pub tokens: Option<TaskTokensDto>,
}

/// `GET /v1/plugins` — one marketplace-installed plugin, resolved from the
/// `claude` CLI (see `crate::plugins`). The webui matches an MCP tool call's
/// server name against `mcp_servers` to enrich the detail view.
#[derive(Serialize)]
pub struct PluginDto {
    /// `plugin@marketplace`.
    pub id: String,
    pub plugin: String,
    pub marketplace: String,
    /// `official` | `public` | `personal` | `unknown`.
    pub provenance: String,
    pub scope: String,
    pub enabled: bool,
    pub mcp_servers: Vec<String>,
    pub description: Option<String>,
}
