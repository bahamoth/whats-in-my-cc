use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Actor {
    User,
    Assistant,
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

impl Default for Actor {
    fn default() -> Self {
        Actor::System
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
            EventKind::Unknown => "unknown",
        }
    }
}

impl Default for EventKind {
    fn default() -> Self {
        EventKind::Unknown
    }
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
    pub payload: serde_json::Value,
    pub parser_version: String,
}
