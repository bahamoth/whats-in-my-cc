//! Canonical rule-id registry for the episode phase classifier (slice-12).
//!
//! Each entry is a versioned rule-id in the form `"phase_<name>@v<N>"`.
//! The list is frozen here and asserted by `tests/episode_rule_registry.rs`.
//! Bumping any rule requires a `@v2` rename and a golden-update commit.

/// Canonical list of versioned rule-ids used by the episode state machine.
/// The order is fixed and must not change without a `@v2` bump.
/// Index mapping (used in `classifier::phase_basis_confidence`):
///   0 = intake, 1 = exploration, 2 = diagnosis, 3 = action,
///   4 = verification, 5 = repair, 6 = drift.
pub const RULE_IDS: &[&str] = &[
    "phase_intake_fresh_user_message@v1",
    "phase_exploration_read_only_window@v1",
    "phase_diagnosis_after_error@v1",
    "phase_action_first_mutation@v1",
    "phase_verification_run_window@v1",
    "phase_repair_after_failed_verification@v1",
    "phase_drift_long_exploration@v1",
];
