//! VerificationRun extractor (slice-11).
//!
//! Walks `ObservedEvent` slices and emits `VerificationRunRecord` rows for
//! events that represent verification activity:
//!   - Bash branch: `ToolCall` with a command on the `verification_allowlist`
//!     + its paired `ToolResult`.
//!   - Hook branch: `HookEvent` with `subkind == "PostToolUse"` and the
//!     matched Bash command on the allowlist (deduped by trigger_event_id).
//!   - OTel branch: `OtelSpan` with `attributes["verification.kind"]` —
//!     see DEV-S11-01; emits no rows in production until real fixture exists.

use crate::model::observed::ObservedEvent;

/// A single verification run row ready for DB insertion.
#[derive(Debug, Clone)]
pub struct VerificationRunRecord {
    pub verification_run_id: String,
    pub schema_version: &'static str,
    pub session_id: String,
    pub source: String,          // "bash" | "hook" | "otel"
    pub command: String,
    pub command_kind: String,
    pub trigger_event_id: String,
    pub trigger_tool_use_id: Option<String>,
    pub status: String,          // "passed" | "failed" | "unknown"
    pub started_at: String,      // ISO 8601 UTC
    pub ended_at: Option<String>,
    pub exit_code: Option<i32>,
    pub failure_summary: Option<String>,
    pub raw_event_id: String,
    pub parser_version: &'static str,
}

/// Extract verification run records from an ordered slice of observed events.
/// Events must belong to the same session.
pub fn extract_verification_runs(_evs: &[ObservedEvent]) -> Vec<VerificationRunRecord> {
    Vec::new()
}
