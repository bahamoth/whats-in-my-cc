//! `Detector` trait — the contract every deterministic detector implements.
//! Pure CPU work; no DB calls, no I/O, no side effects inside `detect()`.

use crate::insight::config::DetectorConfig;
use crate::insight::manifest::DetectorManifest;
use crate::insight::types::SignalCandidate;
use crate::insight::view::SessionInsightView;

/// Implemented by each deterministic detector (구 L1 extractor category).
pub trait Detector: Send + Sync {
    /// Stable detector id (구 category) — appears in `signal.detector`.
    fn id(&self) -> &'static str;

    /// Pure CPU detection. Deterministic. No severity/confidence — facts only.
    /// Same input always produces the same output (idempotent).
    fn detect(&self, view: &SessionInsightView<'_>, cfg: &DetectorConfig) -> Vec<SignalCandidate>;

    /// LLM-readable self-description of this detector (spec §6.4).
    ///
    /// Returns a static declaration of what the detector detects, which raw
    /// payload fields it reads, by what rule, and why. The `inputs` and
    /// `config_keys` fields MUST match the actual `detect()` implementation —
    /// this is the manifest↔predicate contract enforced by `detector_manifest`
    /// integration tests.
    fn manifest(&self) -> DetectorManifest;
}
