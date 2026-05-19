use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct RawEvent {
    pub raw_event_id: String,
    pub ingest_run_id: String,
    pub source_type: String, // "claude_transcript" | "unparseable"
    pub source_uri: String,
    pub source_line_no: i64,
    pub source_byte_offset: i64,
    pub payload_sha256: String,
    pub payload: Vec<u8>,
    pub parse_error: Option<String>,
    pub captured_at: DateTime<Utc>,
}
