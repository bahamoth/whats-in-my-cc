//! Core types for the insight detector pipeline (Plan 1: finding → signal).
//!
//! `SignalCandidate` is the output of a `Detector::detect()` call. It is *not* a
//! `signal` row yet — `pipeline::build_signal_row` derives the id + provenance
//! and `INSERT OR REPLACE`s it before it reaches the DB. NO severity/confidence:
//! those are judgments (spec §6.3); detectors emit deterministic facts only.

/// A deterministic signal produced by a detector. NO severity/confidence —
/// those are judgments (spec §6.3). Only facts. evidence_refs must be non-empty.
#[derive(Debug, Clone)]
pub struct SignalCandidate {
    /// Stable detector id (구 category) — appears in `signal.detector`.
    pub detector: &'static str,
    /// Optional signal sub-type — fact classification only, not interpretation.
    pub subkind: Option<&'static str>,
    /// Factual summary for display (no judgment words).
    pub summary: String,
    /// Event IDs that anchor this signal (must be non-empty per AC-4).
    pub evidence_refs: Vec<String>,
    /// Deterministic facts projection (no severity/confidence).
    pub facts: serde_json::Value,
}

/// Provenance carried by every stored signal.
///
/// `detector` is owned (`String`): it is built per signal as `"<id>@v1"` and
/// serialized once, so a `&'static str` here would have forced a per-signal
/// `Box::leak` — an unbounded leak across the per-ingest `run_detectors`
/// re-runs of a long-lived `serve`. `version` stays `"L1"` (deterministic).
#[derive(Debug, Clone)]
pub struct Provenance {
    /// Version-stamped detector name, e.g. `"tool_failure@v1"`.
    pub detector: String,
    /// Deterministic version tag; always `"L1"` (no judge layer).
    pub version: &'static str,
    /// `None` for code-default detectors (TOML rule_pack id when loaded).
    pub rule_pack: Option<String>,
}

impl Provenance {
    /// Serialize to the JSON shape stored in `signal.provenance`.
    pub fn to_json_string(&self) -> String {
        serde_json::json!({
            "detector": self.detector,
            "version": self.version,
            "rule_pack": self.rule_pack,
        })
        .to_string()
    }
}
