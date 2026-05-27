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

/// Which rule applies for promoting a candidate to a stored Finding.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PromotionPolicy {
    /// L1 always promotes directly (no judge required).
    Always,
    /// L1 never alone promotes; requires L2 judge (judge disabled → dropped).
    Never,
    /// L1 promotes alone if `confidence_l1 > threshold`, otherwise queues for judge.
    IfAbove(f32),
}

/// Provenance carried by every stored Finding.
#[derive(Debug, Clone)]
pub struct Provenance {
    /// Version-stamped extractor name, e.g. `"missing_verification@v1"`.
    pub extractor: &'static str,
    /// `"L1"` for deterministic; `"L2"` for judge-gated.
    pub layer: &'static str,
    /// `None` for L1 findings. `Some("model_id")` when a judge ran.
    pub judge: Option<String>,
    /// `None` for L1 findings.
    pub judge_template_version: Option<String>,
    /// `None` for L1 findings (slice-18 adds redaction rule_pack).
    pub rule_pack: Option<String>,
}

impl Provenance {
    /// Serialize to the JSON shape stored in `finding.provenance`.
    pub fn to_json_string(&self) -> String {
        serde_json::json!({
            "extractor": self.extractor,
            "layer": self.layer,
            "judge": self.judge,
            "judge_template_version": self.judge_template_version,
            "rule_pack": self.rule_pack,
        })
        .to_string()
    }
}
