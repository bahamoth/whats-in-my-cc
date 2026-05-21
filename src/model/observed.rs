use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Actor {
    User,
    Assistant,
    #[default]
    System,
    Hook,
    Tool,
}

impl Actor {
    pub fn as_str(&self) -> &'static str {
        match self {
            Actor::User => "user",
            Actor::Assistant => "assistant",
            Actor::System => "system",
            Actor::Hook => "hook",
            Actor::Tool => "tool",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, strum::EnumIter)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    UserMessage,
    AssistantMessage,
    Thinking,
    ToolCall,
    ToolResult,
    HookEvent,
    SystemSummary,
    SessionState,
    FileHistorySnapshot,
    AttachmentMeta,
    OtelSpan,
    FileEvent,
    GitCommit,
    DiffHunk,
    MetricSample,
    LogRecord,
    #[default]
    Unknown,
}

impl EventKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            EventKind::UserMessage => "user_message",
            EventKind::AssistantMessage => "assistant_message",
            EventKind::Thinking => "thinking",
            EventKind::ToolCall => "tool_call",
            EventKind::ToolResult => "tool_result",
            EventKind::HookEvent => "hook_event",
            EventKind::SystemSummary => "system_summary",
            EventKind::SessionState => "session_state",
            EventKind::FileHistorySnapshot => "file_history_snapshot",
            EventKind::AttachmentMeta => "attachment_meta",
            EventKind::OtelSpan => "otel_span",
            EventKind::FileEvent => "file_event",
            EventKind::GitCommit => "git_commit",
            EventKind::DiffHunk => "diff_hunk",
            EventKind::MetricSample => "metric_sample",
            EventKind::LogRecord => "log_record",
            EventKind::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TelemetryFacet {
    pub span_name: String,
    pub span_kind: Option<String>,
    pub status_code: Option<String>,
    pub status_message: Option<String>,
    pub start_unix_nano: i64,
    pub end_unix_nano: i64,
    #[serde(default)]
    pub attributes: Value,
    #[serde(default)]
    pub resource: Value,
    pub scope_name: Option<String>,
    pub scope_version: Option<String>,
}

/// slice-6 — OTel metric data-point facet. Lives inside `ObservedEvent.payload`
/// as JSON; no dedicated column. One ObservedEvent per data point.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetricFacet {
    pub instrument_name: String,
    /// `sum` | `gauge` | `histogram` | `exponential_histogram` | `summary`
    pub instrument_kind: String,
    pub unit: Option<String>,
    pub description: Option<String>,
    /// `cumulative` | `delta` — only meaningful for sum/histogram.
    pub temporality: Option<String>,
    pub is_monotonic: Option<bool>,
    pub value_int: Option<i64>,
    pub value_float: Option<f64>,
    /// Raw datapoint for histogram / exponentialHistogram / summary so source is preserved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub histogram: Option<Value>,
    #[serde(default)]
    pub attributes: Value,
    #[serde(default)]
    pub resource: Value,
    pub scope_name: Option<String>,
    pub scope_version: Option<String>,
    pub time_unix_nano: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_time_unix_nano: Option<i64>,
}

/// slice-6 — OTel log record facet. Lives inside `ObservedEvent.payload`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LogFacet {
    pub severity_number: Option<i32>,
    pub severity_text: Option<String>,
    /// OTLP "body" is AnyValue; preserved verbatim.
    #[serde(default)]
    pub body: Value,
    /// Pulled out of attributes for indexing convenience.
    pub event_name: Option<String>,
    #[serde(default)]
    pub attributes: Value,
    #[serde(default)]
    pub resource: Value,
    pub scope_name: Option<String>,
    pub scope_version: Option<String>,
    pub time_unix_nano: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_time_unix_nano: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct ObservedEvent {
    pub event_id: String,
    pub raw_event_id: String,
    pub schema_version: String,
    pub session_id: String,
    pub event_uuid: Option<String>,
    pub parent_uuid: Option<String>,
    pub observed_at: DateTime<Utc>,
    pub actor: Actor,
    pub kind: EventKind,
    pub subkind: Option<String>,
    pub tool_use_id: Option<String>,
    pub tool_name: Option<String>,
    pub request_id: Option<String>,
    pub message_id: Option<String>,
    pub turn_id: Option<String>,
    pub source_tool_assistant_uuid: Option<String>,
    pub source_tool_use_id: Option<String>,
    pub is_sidechain: bool,
    pub is_meta: bool,
    pub cwd: Option<String>,
    pub git_branch: Option<String>,
    pub user_type: Option<String>,
    pub entrypoint: Option<String>,
    pub cc_version: Option<String>,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub parent_span_id: Option<String>,
    pub latency_ms: Option<i64>,
    pub telemetry: Option<TelemetryFacet>,
    pub payload: Value,
    pub parser_version: String,
}
