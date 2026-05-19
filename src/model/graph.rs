use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
pub struct GraphNode {
    pub node_id: String,
    pub schema_version: String,
    pub session_id: String,
    pub node_kind: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub merge_keys: Value,
    pub source_event_ids: Vec<String>,
    pub source_uris: Vec<String>,
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct GraphEdge {
    pub edge_id: String,
    pub schema_version: String,
    pub session_id: String,
    pub from_node_id: String,
    pub to_node_id: String,
    pub edge_kind: String,
    pub origin: String,             // "deterministic"
    pub attributes: Value,
}
