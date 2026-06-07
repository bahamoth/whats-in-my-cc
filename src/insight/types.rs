//! Core types for the insight extractor pipeline (slice-14).
//!
//! `FindingCandidate` is the output of an L1 extractor's `extract()` call.
//! It is *not* a `Finding` row yet — it goes through the pipeline
//! (confidence floor, dedup, INSERT OR REPLACE) before reaching the DB.

/// A candidate finding produced by an `InsightExtractor`.
/// The pipeline promotes this to a DB row if `confidence_l1 >= 0.5`.
#[derive(Debug, Clone)]
pub struct FindingCandidate {
    /// Stable category string — appears in `Finding.category`.
    pub category: &'static str,
    /// Optional finding sub-type, persisted to `finding.subkind`. For
    /// `tool_failure` this is the `FailureClass` string; `None` otherwise.
    pub subkind: Option<&'static str>,
    /// L1 confidence; fixed per category (not per-instance).
    pub confidence_l1: f32,
    /// Severity string: `"high"` | `"medium"` | `"low"`.
    pub severity: &'static str,
    /// Human-readable summary for display.
    pub summary: String,
    /// Event IDs that anchor this finding (must be non-empty per AC-4).
    pub evidence_refs: Vec<String>,
    /// JSON projection of the evidence — stored verbatim on the Finding row
    /// for auditability. For L1 categories this is the L1-side projection
    /// (no judge involvement).
    pub evidence_projection: serde_json::Value,
}

/// Provenance carried by every stored Finding.
///
/// The judge subsystem was removed (see #judge-removal), so this carries no
/// `judge` / `judge_template_version` fields — every finding is deterministic
/// L1. `extractor` is owned (`String`): it is built per finding as
/// `"<category>@v1"` and serialized once, so a `&'static str` here would have
/// forced a per-finding `Box::leak` — an unbounded leak across the per-ingest
/// `run_extractors` re-runs of a long-lived `serve`.
#[derive(Debug, Clone)]
pub struct Provenance {
    /// Version-stamped extractor name, e.g. `"missing_verification@v1"`.
    pub extractor: String,
    /// Deterministic layer tag; always `"L1"` now that the judge layer is gone.
    pub layer: &'static str,
    /// `None` for L1 findings (slice-18 adds redaction rule_pack).
    pub rule_pack: Option<String>,
}

impl Provenance {
    /// Serialize to the JSON shape stored in `finding.provenance`.
    pub fn to_json_string(&self) -> String {
        serde_json::json!({
            "extractor": self.extractor,
            "layer": self.layer,
            "rule_pack": self.rule_pack,
        })
        .to_string()
    }
}
