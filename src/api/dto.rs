use serde::Serialize;
use serde_json::Value;

#[derive(Serialize)]
pub struct SessionListItem {
    pub session_id: String,
    pub first_observed_at: String,
    pub last_observed_at: String,
    pub event_count: i64,
    pub source_uris: Vec<String>, // slice-1: empty array (tracked at observed_event payload level)
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
}
