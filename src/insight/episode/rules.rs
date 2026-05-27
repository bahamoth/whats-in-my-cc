//! Canonical rule-id registry for the episode phase classifier (slice-12).
//!
//! Each entry is a versioned rule-id in the form `"phase_<name>@v<N>"`.
//! The list is frozen here and asserted by `tests/episode_rule_registry.rs`.
//! Bumping any rule requires a `@v2` rename and a golden-update commit.

/// Canonical list of versioned rule-ids used by the episode state machine.
/// The order matters — it is the order rules are checked inside the machine.
pub const RULE_IDS: &[&str] = &[];
