//! `Detector` trait — the contract every deterministic detector implements.
//! Pure CPU work; no DB calls, no I/O, no side effects inside `detect()`.

use crate::insight::config::DetectorConfig;
use crate::insight::types::SignalCandidate;
use crate::insight::view::SessionInsightView;

/// Implemented by each deterministic detector (구 L1 extractor category).
pub trait Detector: Send + Sync {
    /// Stable detector id (구 category) — appears in `signal.detector`.
    fn id(&self) -> &'static str;

    /// Pure CPU detection. Deterministic. No severity/confidence — facts only.
    /// Same input always produces the same output (idempotent).
    fn detect(&self, view: &SessionInsightView<'_>, cfg: &DetectorConfig) -> Vec<SignalCandidate>;
}
