//! Slice-18 — RedactionManifest struct and serialisation.
//!
//! Schema version: "redaction_manifest.v1"
//! Rule pack:      "rule_pack@v1"

use serde::{Deserialize, Serialize};

/// Canonical schema version for this manifest shape.
pub const MANIFEST_SCHEMA_VERSION: &str = "redaction_manifest.v1";
/// Canonical rule pack identifier.
pub const RULE_PACK_ID: &str = "rule_pack@v1";

/// State of the payload after the redaction gate ran.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RedactionState {
    /// Gate ran, ≥1 item was masked.
    Redacted,
    /// Gate ran, nothing was masked.
    NotRedacted,
    /// Gate was not applicable (e.g., binary or empty payload).
    NotApplicable,
}

impl RedactionState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Redacted => "redacted",
            Self::NotRedacted => "not_redacted",
            Self::NotApplicable => "not_applicable",
        }
    }
}

/// Per-event redaction manifest written to `raw_event.redaction_manifest`.
///
/// Serialised to JSON and stored as TEXT. Aggregated by the API response layer
/// to produce `meta.redaction_summary`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedactionManifest {
    pub schema_version: &'static str,
    pub rule_pack: &'static str,
    pub redaction_state: RedactionState,
    /// Rule IDs that matched at least once in this payload.
    pub rules_applied: Vec<String>,
    /// Total count of distinct match replacements made.
    pub items_redacted_count: u32,
    /// True when the high-entropy heuristic fired but no specific rule did.
    /// Signals that the payload may still contain unmasked sensitive data
    /// and should be reviewed before export.
    pub has_unredacted_sensitive_payload: bool,
    /// True when `has_unredacted_sensitive_payload` is true.
    pub review_required_before_export: bool,
}

impl RedactionManifest {
    pub fn not_redacted() -> Self {
        Self {
            schema_version: MANIFEST_SCHEMA_VERSION,
            rule_pack: RULE_PACK_ID,
            redaction_state: RedactionState::NotRedacted,
            rules_applied: vec![],
            items_redacted_count: 0,
            has_unredacted_sensitive_payload: false,
            review_required_before_export: false,
        }
    }

    pub fn not_applicable() -> Self {
        Self {
            schema_version: MANIFEST_SCHEMA_VERSION,
            rule_pack: RULE_PACK_ID,
            redaction_state: RedactionState::NotApplicable,
            rules_applied: vec![],
            items_redacted_count: 0,
            has_unredacted_sensitive_payload: false,
            review_required_before_export: false,
        }
    }
}
