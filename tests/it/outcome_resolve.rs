//! Tests for `resolve_outcome` (Plan 6) — OTLP-first fallback chain.
//!
//! Key invariants:
//! - is_error=false + no OTLP → Unknown, NOT Passed.
//! - OTLP success=false → Failed/Measured.
//! - content "exit code: 2" → Failed/Measured.
//! - content "Exit code 7" (real CC prepend, no colon) → Failed/Measured.
//! - Hook exit_code=0 → Passed/Measured.
//! - Fallback order: OTLP > hook > content.

use chrono::{TimeZone, Utc};
use serde_json::json;
use wimcc::insight::outcome::{resolve_outcome, OutcomeProvenance, OutcomeStatus};
use wimcc::model::observed::{Actor, EventKind, ObservedEvent};

fn ts(i: i64) -> chrono::DateTime<Utc> {
    Utc.timestamp_opt(1_700_000_000 + i * 10, 0).unwrap()
}

fn mk_event(i: i64, kind: EventKind, payload: serde_json::Value) -> ObservedEvent {
    ObservedEvent {
        event_id: format!("ev_{i:03}"),
        raw_event_id: format!("raw_{i:03}"),
        schema_version: "observed_event.v1".into(),
        session_id: "sess_test".into(),
        observed_at: ts(i),
        actor: Actor::Tool,
        kind,
        parser_version: "test".into(),
        payload,
        ..Default::default()
    }
}

fn tool_result(i: i64, tool_use_id: &str, is_error: bool, content: &str) -> ObservedEvent {
    ObservedEvent {
        tool_use_id: Some(tool_use_id.into()),
        ..mk_event(
            i,
            EventKind::ToolResult,
            json!({
                "tool_result": {
                    "tool_use_id": tool_use_id,
                    "is_error": is_error,
                    "content": content
                }
            }),
        )
    }
}

/// is_error=false + no OTLP/hook/exit-code-content → Unknown, NOT Passed.
/// This is the key regression guard for the original bug.
#[test]
fn is_error_false_no_otlp_yields_unknown() {
    let events = vec![tool_result(0, "tid_1", false, "all good")];
    let outcome = resolve_outcome(&events, "tid_1");
    assert_eq!(
        outcome.status,
        OutcomeStatus::Unknown,
        "is_error=false without OTLP/hook/exit-code must be Unknown, not Passed"
    );
    assert_eq!(outcome.provenance, OutcomeProvenance::Unknown);
}

/// is_error=true but still no OTLP → Unknown (is_error is not used for outcome).
#[test]
fn is_error_true_no_otlp_yields_unknown() {
    let events = vec![tool_result(0, "tid_1", true, "something broke")];
    let outcome = resolve_outcome(&events, "tid_1");
    assert_eq!(
        outcome.status,
        OutcomeStatus::Unknown,
        "is_error=true without OTLP/hook/exit-code must be Unknown (is_error is tool-execution only)"
    );
}

/// OTLP log_record with success="false" → Failed/Measured.
#[test]
fn otlp_success_false_yields_failed_measured() {
    let otlp_event = ObservedEvent {
        tool_use_id: Some("tid_1".into()),
        ..mk_event(
            1,
            EventKind::LogRecord,
            json!({
                "event_name": "tool_result",
                "attributes": {
                    "tool_use_id": "tid_1",
                    "success": "false"
                }
            }),
        )
    };
    let events = vec![tool_result(0, "tid_1", false, "build failed"), otlp_event];
    let outcome = resolve_outcome(&events, "tid_1");
    assert_eq!(outcome.status, OutcomeStatus::Failed);
    assert_eq!(outcome.provenance, OutcomeProvenance::Measured);
}

/// OTLP log_record with success="true" → Passed/Measured.
#[test]
fn otlp_success_true_yields_passed_measured() {
    let otlp_event = ObservedEvent {
        tool_use_id: Some("tid_2".into()),
        ..mk_event(
            1,
            EventKind::LogRecord,
            json!({
                "event_name": "tool_result",
                "attributes": {
                    "tool_use_id": "tid_2",
                    "success": "true"
                }
            }),
        )
    };
    let events = vec![otlp_event];
    let outcome = resolve_outcome(&events, "tid_2");
    assert_eq!(outcome.status, OutcomeStatus::Passed);
    assert_eq!(outcome.provenance, OutcomeProvenance::Measured);
}

/// OTLP log_record with wrong event_name → skipped, falls through to Unknown.
#[test]
fn otlp_wrong_event_name_is_skipped() {
    let otlp_event = ObservedEvent {
        tool_use_id: Some("tid_1".into()),
        ..mk_event(
            1,
            EventKind::LogRecord,
            json!({
                "event_name": "hook_execution_complete",
                "attributes": {
                    "tool_use_id": "tid_1",
                    "success": "false"
                }
            }),
        )
    };
    let events = vec![otlp_event];
    let outcome = resolve_outcome(&events, "tid_1");
    assert_eq!(outcome.status, OutcomeStatus::Unknown);
}

/// Hook post_tool_use with exit_code=0 → Passed/Measured.
#[test]
fn hook_exit_code_zero_yields_passed_measured() {
    let hook_event = ObservedEvent {
        tool_use_id: Some("tid_3".into()),
        kind: EventKind::HookEvent,
        subkind: Some("post_tool_use".into()),
        ..mk_event(
            0,
            EventKind::HookEvent,
            json!({
                "hook": {
                    "session_id": "sess_test",
                    "hook_event_name": "PostToolUse",
                    "tool_name": "Bash",
                    "tool_use_id": "tid_3",
                    "tool_response": {"stdout": "ok", "stderr": "", "exit_code": 0}
                }
            }),
        )
    };
    let events = vec![hook_event];
    let outcome = resolve_outcome(&events, "tid_3");
    assert_eq!(outcome.status, OutcomeStatus::Passed);
    assert_eq!(outcome.provenance, OutcomeProvenance::Measured);
}

/// Hook post_tool_use with exit_code=1 → Failed/Measured.
#[test]
fn hook_exit_code_nonzero_yields_failed_measured() {
    let hook_event = ObservedEvent {
        tool_use_id: Some("tid_4".into()),
        kind: EventKind::HookEvent,
        subkind: Some("post_tool_use".into()),
        ..mk_event(
            0,
            EventKind::HookEvent,
            json!({
                "hook": {
                    "session_id": "sess_test",
                    "hook_event_name": "PostToolUse",
                    "tool_name": "Bash",
                    "tool_use_id": "tid_4",
                    "tool_response": {"stdout": "", "stderr": "error", "exit_code": 2}
                }
            }),
        )
    };
    let events = vec![hook_event];
    let outcome = resolve_outcome(&events, "tid_4");
    assert_eq!(outcome.status, OutcomeStatus::Failed);
    assert_eq!(outcome.provenance, OutcomeProvenance::Measured);
}

/// Hook event with wrong subkind (pre_tool_use) → skipped.
#[test]
fn hook_pre_tool_use_is_skipped() {
    let hook_event = ObservedEvent {
        tool_use_id: Some("tid_5".into()),
        kind: EventKind::HookEvent,
        subkind: Some("pre_tool_use".into()),
        ..mk_event(
            0,
            EventKind::HookEvent,
            json!({
                "hook": {
                    "tool_use_id": "tid_5",
                    "tool_response": {"exit_code": 0}
                }
            }),
        )
    };
    let events = vec![hook_event];
    let outcome = resolve_outcome(&events, "tid_5");
    assert_eq!(outcome.status, OutcomeStatus::Unknown);
}

/// Content "exit code: 2" → Failed/Measured (structural parse, not heuristic).
#[test]
fn content_exit_code_nonzero_yields_failed_measured() {
    let events = vec![tool_result(
        0,
        "tid_6",
        false, // is_error=false (the original bug scenario)
        "FAILED: test_something\n\nexit code: 2",
    )];
    let outcome = resolve_outcome(&events, "tid_6");
    assert_eq!(
        outcome.status,
        OutcomeStatus::Failed,
        "explicit 'exit code: 2' in content must resolve to Failed"
    );
    assert_eq!(outcome.provenance, OutcomeProvenance::Measured);
}

/// Content "Exit Code: 0" (case insensitive) → Passed/Measured.
#[test]
fn content_exit_code_zero_yields_passed_measured() {
    let events = vec![tool_result(
        0,
        "tid_7",
        false,
        "All tests passed.\nExit Code: 0",
    )];
    let outcome = resolve_outcome(&events, "tid_7");
    assert_eq!(outcome.status, OutcomeStatus::Passed);
    assert_eq!(outcome.provenance, OutcomeProvenance::Measured);
}

/// REAL Claude Code format: on a non-zero Bash exit, CC prepends "Exit code <N>\n"
/// (capital E, NO colon) to the tool_result content and sets is_error=true. This
/// content is frozen verbatim from a real session (c8256e80, command `… ; exit 7`);
/// the same prepend form was confirmed across 215 local sessions / 82 occurrences
/// spanning CC 2.1.153–2.1.168. The prior colon-only matcher missed every one of
/// these, so genuine measured failures silently resolved to Unknown.
#[test]
fn content_cc_exit_code_prepend_yields_failed_measured() {
    let events = vec![tool_result(
        0,
        "tid_cc",
        true, // CC sets is_error=true alongside the "Exit code N" prepend
        "Exit code 7\nOTLP measured-outcome verification: intentional non-zero exit",
    )];
    let outcome = resolve_outcome(&events, "tid_cc");
    assert_eq!(
        outcome.status,
        OutcomeStatus::Failed,
        "real CC 'Exit code 7' prepend must resolve to Failed, not Unknown"
    );
    assert_eq!(outcome.provenance, OutcomeProvenance::Measured);
}

/// Fallback order: OTLP takes priority over hook.
#[test]
fn otlp_beats_hook_in_fallback_order() {
    let otlp_event = ObservedEvent {
        tool_use_id: Some("tid_8".into()),
        ..mk_event(
            1,
            EventKind::LogRecord,
            json!({
                "event_name": "tool_result",
                "attributes": { "tool_use_id": "tid_8", "success": "false" }
            }),
        )
    };
    let hook_event = ObservedEvent {
        tool_use_id: Some("tid_8".into()),
        kind: EventKind::HookEvent,
        subkind: Some("post_tool_use".into()),
        ..mk_event(
            2,
            EventKind::HookEvent,
            json!({
                "hook": {
                    "tool_use_id": "tid_8",
                    "tool_response": {"exit_code": 0}
                }
            }),
        )
    };
    // OTLP says Failed, hook says Passed — OTLP wins.
    let events = vec![tool_result(0, "tid_8", false, ""), otlp_event, hook_event];
    let outcome = resolve_outcome(&events, "tid_8");
    assert_eq!(
        outcome.status,
        OutcomeStatus::Failed,
        "OTLP must take priority over hook"
    );
    assert_eq!(outcome.provenance, OutcomeProvenance::Measured);
}

/// Only matches events for the exact tool_use_id (no cross-contamination).
#[test]
fn does_not_match_different_tool_use_id() {
    let otlp_event = ObservedEvent {
        tool_use_id: Some("tid_other".into()),
        ..mk_event(
            1,
            EventKind::LogRecord,
            json!({
                "event_name": "tool_result",
                "attributes": { "tool_use_id": "tid_other", "success": "false" }
            }),
        )
    };
    let events = vec![tool_result(0, "tid_target", false, "ok"), otlp_event];
    let outcome = resolve_outcome(&events, "tid_target");
    assert_eq!(
        outcome.status,
        OutcomeStatus::Unknown,
        "must not use OTLP for different tool_use_id"
    );
}
