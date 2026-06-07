//! Command outcome resolution (Plan 6): OTLP-first + fallback chain.
//!
//! `is_error` in the transcript is **tool-execution level only** (did the tool
//! executor accept the call?) — it does NOT reflect the command's exit code.
//! A `cargo test` that prints "FAILED" and exits 1 has is_error=false because
//! the Bash tool itself ran successfully.
//!
//! Fallback chain (first match wins):
//! 1. OTLP `log_record` (event_name=tool_result, same tool_use_id) → `attributes.success`
//!    — measured.
//! 2. Hook `post_tool_use` (same tool_use_id) → `tool_response.exit_code`
//!    — measured.
//! 3. Transcript `tool_result` content with explicit "exit code: N" text
//!    — measured (structural parse, not heuristic).
//! 4. Nothing matched → Unknown (is_error is NOT used for outcome).

use crate::model::observed::{EventKind, ObservedEvent};

/// Resolved command outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeStatus {
    Passed,
    Failed,
    Unknown,
}

/// How the outcome was determined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeProvenance {
    /// Derived from a signal that directly reflects the command's exit
    /// (OTLP success attribute or hook exit_code or explicit "exit code: N").
    Measured,
    /// Derived from a tool-specific output pattern heuristic (e.g. "FAILED").
    Estimated,
    /// No measured signal available; outcome is genuinely unknown.
    Unknown,
}

/// Resolved outcome pair.
#[derive(Debug, Clone, Copy)]
pub struct Outcome {
    pub status: OutcomeStatus,
    pub provenance: OutcomeProvenance,
}

impl Outcome {
    pub const UNKNOWN: Outcome = Outcome {
        status: OutcomeStatus::Unknown,
        provenance: OutcomeProvenance::Unknown,
    };
}

/// Resolve the command outcome for a given `tool_use_id` from the event slice.
///
/// Events must belong to the same session. Order is not assumed (all three
/// steps scan the full slice).
///
/// The `is_error` field of transcript `tool_result` events is intentionally
/// **not** used to determine pass/fail — it only indicates whether the tool
/// executor accepted the invocation.
pub fn resolve_outcome(events: &[ObservedEvent], tool_use_id: &str) -> Outcome {
    // ── Step 1: OTLP log_record (event_name=tool_result) ─────────────────────
    // Payload shape: LogFacet serialised to JSON.
    //   { "event_name": "tool_result",
    //     "attributes": { "tool_use_id": "...", "success": "true"|"false", ... },
    //     ... }
    // `event_name` is promoted out of OTLP attributes by the log ingest layer
    // (otel_logs.rs:72-75) into a top-level LogFacet field.
    // `attributes` is the flattened OTLP attribute map.
    for ev in events {
        if ev.kind != EventKind::LogRecord {
            continue;
        }
        if ev.tool_use_id.as_deref() != Some(tool_use_id) {
            continue;
        }
        let is_tool_result_log = ev
            .payload
            .pointer("/event_name")
            .and_then(|v| v.as_str())
            == Some("tool_result");
        if !is_tool_result_log {
            continue;
        }
        if let Some(success_str) = ev
            .payload
            .pointer("/attributes/success")
            .and_then(|v| v.as_str())
        {
            let status = if success_str == "true" {
                OutcomeStatus::Passed
            } else {
                OutcomeStatus::Failed
            };
            return Outcome {
                status,
                provenance: OutcomeProvenance::Measured,
            };
        }
    }

    // ── Step 2: Hook post_tool_use exit_code ──────────────────────────────────
    // Payload shape: {"hook": <original Claude Code hook JSON>}
    // The hook JSON has `tool_use_id` at the top level and `tool_response` with
    // `exit_code`. (Verified against tests/fixtures/hook/post_tool_use.json.)
    for ev in events {
        if ev.kind != EventKind::HookEvent {
            continue;
        }
        if ev.subkind.as_deref() != Some("post_tool_use") {
            continue;
        }
        // The tool_use_id is indexed in ObservedEvent.tool_use_id by hook ingest.
        if ev.tool_use_id.as_deref() != Some(tool_use_id) {
            continue;
        }
        if let Some(code) = ev
            .payload
            .pointer("/hook/tool_response/exit_code")
            .and_then(|v| v.as_i64())
        {
            let status = if code == 0 {
                OutcomeStatus::Passed
            } else {
                OutcomeStatus::Failed
            };
            return Outcome {
                status,
                provenance: OutcomeProvenance::Measured,
            };
        }
    }

    // ── Step 3: Transcript tool_result content "exit code: N" ─────────────────
    // Payload shape: {"tool_result": {"content": "...", ...}}
    // Only matches explicit "exit code: <digits>" text (structural, not
    // heuristic — does not match "non-zero exit" or similar prose).
    for ev in events {
        if ev.kind != EventKind::ToolResult {
            continue;
        }
        if ev.tool_use_id.as_deref() != Some(tool_use_id) {
            continue;
        }
        if let Some(content) = ev
            .payload
            .pointer("/tool_result/content")
            .and_then(|v| v.as_str())
        {
            if let Some(code) = parse_exit_code(content) {
                let status = if code == 0 {
                    OutcomeStatus::Passed
                } else {
                    OutcomeStatus::Failed
                };
                return Outcome {
                    status,
                    provenance: OutcomeProvenance::Measured,
                };
            }
        }
    }

    // ── No signal found ───────────────────────────────────────────────────────
    Outcome::UNKNOWN
}

/// Parse an explicit "exit code: N" from tool output content.
///
/// Matches case-insensitively. Requires the literal text "exit code:" followed
/// by optional whitespace and one or more ASCII digits. Returns the parsed
/// integer if found.
///
/// This is a structural parse, not a heuristic — it does NOT match prose like
/// "returned non-zero exit status" or "exit status: 1".
pub fn parse_exit_code(content: &str) -> Option<i64> {
    let lc = content.to_ascii_lowercase();
    let needle = "exit code:";
    let pos = lc.find(needle)?;
    let after = content[pos + needle.len()..].trim_start();
    let digits: &str = after
        .split(|c: char| !c.is_ascii_digit())
        .next()
        .filter(|s| !s.is_empty())?;
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_exit_code_basic() {
        assert_eq!(parse_exit_code("exit code: 2"), Some(2));
        assert_eq!(parse_exit_code("Exit Code: 0"), Some(0));
        assert_eq!(parse_exit_code("EXIT CODE:127"), Some(127));
        assert_eq!(parse_exit_code("no exit here"), None);
        assert_eq!(parse_exit_code("exit code: abc"), None);
        assert_eq!(parse_exit_code("exit code:"), None);
        // Does not match prose
        assert_eq!(parse_exit_code("returned non-zero exit status 2"), None);
    }

    #[test]
    fn parse_exit_code_in_multiline() {
        let s = "FAILED\n\nexit code: 1\nsome more output";
        assert_eq!(parse_exit_code(s), Some(1));
    }

    #[test]
    fn parse_exit_code_with_leading_spaces() {
        assert_eq!(parse_exit_code("exit code:  42"), Some(42));
    }
}
