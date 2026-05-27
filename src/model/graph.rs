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
    /// `"deterministic"` for edges built by `compute()`; `"inferred"` for edges
    /// produced by the inference rules in `src/insight/edge_inference/`.
    pub origin: String,
    pub attributes: Value,
    /// Versioned rule ID that produced this edge, e.g. `"caused_repair@v1"`.
    /// `None` for deterministic edges.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inference_rule_id: Option<String>,
    /// Numeric confidence in `[0.0, 1.0]`. `None` for deterministic edges.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
}
