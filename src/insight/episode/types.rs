//! Episode type definitions (slice-12).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// The seven phase labels defined in `docs/03_data_model_spec.html` §6.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Intake,
    Exploration,
    Diagnosis,
    Action,
    Verification,
    Repair,
    Drift,
}

/// An episode produced by the phase classifier.
///
/// Each field maps 1:1 to a column in the `episode` table.
/// `classification_basis` entries are versioned rule-ids from `rules::RULE_IDS`.
#[derive(Debug, Clone)]
pub struct EpisodeRecord {
    pub episode_id: String,
    pub schema_version: String,
    pub session_id: String,
    pub phase: Phase,
    pub start_event_id: String,
    pub end_event_id: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    /// Serialised as JSON array in the DB column `evidence_node_ids`.
    pub evidence_node_ids: Vec<String>,
    /// Versioned rule-ids that justify the label; stored as JSON array.
    pub classification_basis: Vec<&'static str>,
    pub confidence: f32,
    pub summary: Option<String>,
    pub classifier_version: String,
}
