//! Plan 6, Task 2 — verification_run extractor must use resolve_outcome, not is_error.
//!
//! Key regressions guarded:
//! - is_error=false + no OTLP → status="unknown" (NOT "passed"). This is the
//!   core bug: cargo test that fails exits 1 but is_error stays false.
//! - is_error=false + content "exit code: 2" → status="failed" (Measured).
//! - is_error=false + content "exit code: 0" → status="passed" (Measured).
//! - is_error=false + OTLP success=false → status="failed" (Measured).
//! - status_provenance column is present in the VerificationRunRecord.

use chrono::{TimeZone, Utc};
use serde_json::json;
use wimcc::ingest::verification_run::extract_verification_runs;
use wimcc::model::observed::{Actor, EventKind, ObservedEvent};

fn ts(i: i64) -> chrono::DateTime<Utc> {
    Utc.timestamp_opt(1_700_000_000 + i * 10, 0).unwrap()
}

fn tool_call_bash(i: i64, tid: &str, cmd: &str) -> ObservedEvent {
    ObservedEvent {
        event_id: format!("ev_{i:03}"),
        raw_event_id: format!("raw_{i:03}"),
        schema_version: "observed_event.v1".into(),
        session_id: "sess_vr".into(),
        observed_at: ts(i),
        actor: Actor::Assistant,
        kind: EventKind::ToolCall,
        tool_use_id: Some(tid.into()),
        tool_name: Some("Bash".into()),
        parser_version: "test".into(),
        payload: json!({
            "tool_use_id": tid,
            "name": "Bash",
            "input": {"command": cmd}
        }),
        ..Default::default()
    }
}

fn tool_result_ev(i: i64, tid: &str, is_error: bool, content: &str) -> ObservedEvent {
    ObservedEvent {
        event_id: format!("ev_{i:03}"),
        raw_event_id: format!("raw_{i:03}"),
        schema_version: "observed_event.v1".into(),
        session_id: "sess_vr".into(),
        observed_at: ts(i),
        actor: Actor::Tool,
        kind: EventKind::ToolResult,
        tool_use_id: Some(tid.into()),
        parser_version: "test".into(),
        payload: json!({
            "tool_result": {
                "tool_use_id": tid,
                "is_error": is_error,
                "content": content
            }
        }),
        ..Default::default()
    }
}

fn otlp_tool_result(i: i64, tid: &str, success: &str) -> ObservedEvent {
    ObservedEvent {
        event_id: format!("ev_{i:03}"),
        raw_event_id: format!("raw_{i:03}"),
        schema_version: "observed_event.v1".into(),
        session_id: "sess_vr".into(),
        observed_at: ts(i),
        actor: Actor::System,
        kind: EventKind::LogRecord,
        tool_use_id: Some(tid.into()),
        parser_version: "test".into(),
        payload: json!({
            "event_name": "tool_result",
            "attributes": {
                "tool_use_id": tid,
                "success": success
            }
        }),
        ..Default::default()
    }
}

/// Core regression: is_error=false with no OTLP/hook/exit-code and no
/// recognisable failure pattern in content → Unknown, NOT Passed.
///
/// Before Plan 6 this would have yielded "passed" (because is_error=false).
/// After Plan 6 the chain returns Unknown when there is no measured signal.
/// (Note: content WITH recognisable failure patterns triggers Tier-4 estimated;
/// this test uses neutral content to isolate the is_error-is-not-a-signal rule.)
#[test]
fn is_error_false_no_otlp_neutral_content_yields_unknown_not_passed() {
    let evs = vec![
        tool_call_bash(0, "tid_1", "cargo test --all"),
        // is_error=false is the normal case; content has no exit-code or failure pattern.
        tool_result_ev(1, "tid_1", false, "running 10 tests\nok"),
    ];
    let runs = extract_verification_runs(&evs);
    assert_eq!(runs.len(), 1);
    let run = &runs[0];
    assert_eq!(
        run.status, "unknown",
        "is_error=false without any measured/estimated signal must yield unknown, not passed; got {:?}",
        run.status
    );
}

/// When content contains a recognisable failure pattern but no OTLP/exit-code,
/// Tier-4 estimated rule fires → Failed/Estimated (not Passed).
/// This confirms the original bug is fixed even in the common case.
#[test]
fn is_error_false_with_failure_content_yields_failed_estimated() {
    let evs = vec![
        tool_call_bash(0, "tid_1b", "cargo test --all"),
        // is_error=false is the normal case for a failing cargo test (the bug).
        tool_result_ev(
            1,
            "tid_1b",
            false,
            "FAILED\n\nerror[E0308]: mismatched types",
        ),
    ];
    let runs = extract_verification_runs(&evs);
    assert_eq!(runs.len(), 1);
    let run = &runs[0];
    assert_eq!(
        run.status, "failed",
        "is_error=false with failure content must yield failed via Tier-4; got {:?}",
        run.status
    );
    assert_eq!(
        run.status_provenance.as_deref(),
        Some("estimated"),
        "Tier-4 rule must produce estimated provenance"
    );
}

/// Content with explicit "exit code: 2" → Failed (Measured), is_error irrelevant.
#[test]
fn content_exit_code_nonzero_yields_failed() {
    let evs = vec![
        tool_call_bash(0, "tid_2", "cargo test"),
        tool_result_ev(1, "tid_2", false, "FAILED\n\nexit code: 2"),
    ];
    let runs = extract_verification_runs(&evs);
    assert_eq!(runs.len(), 1);
    let run = &runs[0];
    assert_eq!(
        run.status, "failed",
        "content 'exit code: 2' must resolve to failed; got {:?}",
        run.status
    );
    assert_eq!(
        run.status_provenance.as_deref(),
        Some("measured"),
        "provenance must be measured when derived from exit code text"
    );
}

/// Content with "exit code: 0" → Passed/Measured.
#[test]
fn content_exit_code_zero_yields_passed() {
    let evs = vec![
        tool_call_bash(0, "tid_3", "cargo build"),
        tool_result_ev(1, "tid_3", false, "Compiling...\nFinished\nexit code: 0"),
    ];
    let runs = extract_verification_runs(&evs);
    assert_eq!(runs.len(), 1);
    let run = &runs[0];
    assert_eq!(run.status, "passed");
    assert_eq!(run.status_provenance.as_deref(), Some("measured"));
}

/// OTLP success=false → Failed/Measured even with is_error=false.
#[test]
fn otlp_success_false_overrides_is_error_false() {
    let evs = vec![
        tool_call_bash(0, "tid_4", "cargo test"),
        tool_result_ev(1, "tid_4", false, "all fine"),
        otlp_tool_result(2, "tid_4", "false"),
    ];
    let runs = extract_verification_runs(&evs);
    assert_eq!(runs.len(), 1);
    let run = &runs[0];
    assert_eq!(run.status, "failed");
    assert_eq!(run.status_provenance.as_deref(), Some("measured"));
}

/// OTLP success=true → Passed/Measured.
#[test]
fn otlp_success_true_yields_passed() {
    let evs = vec![
        tool_call_bash(0, "tid_5", "cargo test"),
        tool_result_ev(1, "tid_5", false, "some output"),
        otlp_tool_result(2, "tid_5", "true"),
    ];
    let runs = extract_verification_runs(&evs);
    assert_eq!(runs.len(), 1);
    let run = &runs[0];
    assert_eq!(run.status, "passed");
    assert_eq!(run.status_provenance.as_deref(), Some("measured"));
}

/// status_provenance field is present in VerificationRunRecord.
/// Tests that the struct itself compiles with the new field.
#[test]
fn verification_run_record_has_status_provenance_field() {
    let evs = vec![
        tool_call_bash(0, "tid_6", "cargo test"),
        tool_result_ev(1, "tid_6", false, ""),
    ];
    let runs = extract_verification_runs(&evs);
    assert_eq!(runs.len(), 1);
    // Just accessing the field must compile — confirms it exists.
    let _prov = runs[0].status_provenance.clone();
}
