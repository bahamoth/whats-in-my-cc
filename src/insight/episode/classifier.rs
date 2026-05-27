//! Episode phase classifier — state machine (slice-12).
//!
//! `classify_session` is a pure function: same input always produces the same
//! output (`tests/episode_determinism.rs` asserts this). No I/O, no globals.

use crate::db::repo_verification_run::VerificationRunRow;
use crate::model::observed::ObservedEvent;

use super::types::EpisodeRecord;

/// Classify an ordered event stream into a sequence of `EpisodeRecord`s.
///
/// `events` must be in `observed_at` order (the caller — graph builder — ensures
/// this). `runs` is the set of `VerificationRunRow`s for the same session; the
/// classifier uses them to emit `Verification` phase episodes.
///
/// Returns an empty `Vec` for an empty event stream (spec §8).
pub fn classify_session(
    _session_id: &str,
    _events: &[ObservedEvent],
    _runs: &[VerificationRunRow],
) -> Vec<EpisodeRecord> {
    // TODO(slice-12 Phase 3): implement state machine
    Vec::new()
}
