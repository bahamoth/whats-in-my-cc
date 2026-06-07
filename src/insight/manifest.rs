//! Detector manifest — the LLM-readable declaration of a detector (spec §6.4).
//!
//! A detector has three layers:
//!   - **predicate** = the Rust code (the authoritative truth)
//!   - **config**    = `DetectorConfig` rule pack (TOML-tunable parameters)
//!   - **manifest**  = this struct (what an LLM reads to understand the detector)
//!
//! The manifest is read-only and exposed via `GET /v1/detectors` and MCP
//! `list_detectors`. It carries no runtime data; it is a static declaration
//! baked into each detector at compile time.
//!
//! Invariant: `manifest.id == detector.id()` and `manifest.inputs` must
//! reference the actual payload fields the predicate reads (code-verified).

/// LLM-readable self-description of a deterministic detector (spec §6.4).
///
/// All fields are `&'static str` or `Vec<&'static str>` — compiled into the
/// binary, zero heap allocation per call.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DetectorManifest {
    /// Stable detector id. Must equal `Detector::id()`.
    pub id: &'static str,

    /// What the detector detects — one sentence, human- and LLM-readable.
    pub intent: &'static str,

    /// Raw payload field paths the predicate actually reads (dot-notation).
    /// Verification: these must match the code in the detector's `detect()`.
    pub inputs: Vec<&'static str>,

    /// Pseudocode / natural-language rule describing the firing condition.
    pub rule: &'static str,

    /// Shape of the emitted signal's `facts` object.
    pub output: &'static str,

    /// `DetectorConfig` parameter keys the predicate reads via `usize_param`.
    /// Empty when the detector ignores config (e.g. no tunable threshold).
    pub config_keys: Vec<&'static str>,

    /// Evidence anchor: docs section URL fragment and/or real fixture path.
    pub rationale: &'static str,
}
