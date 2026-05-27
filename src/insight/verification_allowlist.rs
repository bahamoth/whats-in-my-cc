//! Allowlist of Bash command patterns that indicate a verification run.
//! This list is **closed** per DEV-S11-03: adding a pattern requires a new
//! slice with a real-fixture invariant test.
//!
//! parser_version: "verification_run@v1"

/// Returns the full allowlist as (regex_pattern, command_kind) pairs.
/// The regex patterns are anchored (`^...$`) and use simplified matching for
/// commands that may have trailing arguments.
pub fn allowlist_patterns() -> &'static [(&'static str, &'static str)] {
    &[]
}

/// Returns the `command_kind` for the first matching pattern, or `None` if no
/// pattern matches. The match is performed against the full command string.
pub fn classify(_cmd: &str) -> Option<&'static str> {
    None
}
