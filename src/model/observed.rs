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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
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
