//! Plan 6, Task 3 — ToolFailure detector must use resolve_outcome, not is_error.
//!
//! Key invariants:
//! - is_error=true + no OTLP/hook/exit-code → resolve_outcome returns Unknown
//!   → ToolFailure does NOT fire (Unknown is not a failure signal).
//! - is_error=false + OTLP success=false → resolve_outcome returns Failed
//!   → ToolFailure fires.
//! - is_error=false + content "exit code: 2" → fires with Measured provenance.
//! - is_error is kept in facts but labeled as tool-execution indicator only.
//! - `outcome_provenance` is in facts.

use chrono::{TimeZone, Utc};
use serde_json::json;
use wimcc::insight::config::DetectorConfig;
use wimcc::insight::extractor::Detector;
use wimcc::insight::extractors::tool_failure::ToolFailure;
use wimcc::insight::view::SessionInsightView;
use wimcc::model::observed::{Actor, EventKind, ObservedEvent};

fn ts(i: i64) -> chrono::DateTime<Utc> {
    Utc.timestamp_opt(1_700_000_000 + i * 10, 0).unwrap()
}

fn mk(i: i64, kind: EventKind, payload: serde_json::Value) -> ObservedEvent {
    ObservedEvent {
        event_id: format!("ev_{i:03}"),
        raw_event_id: format!("raw_{i:03}"),
        schema_version: "observed_event.v1".into(),
        session_id: "sess_tf".into(),
        observed_at: ts(i),
        actor: Actor::Tool,
        kind,
        parser_version: "test".into(),
        payload,
        ..Default::default()
    }
}

fn tool_result(i: i64, tid: &str, is_error: bool, content: &str) -> ObservedEvent {
    ObservedEvent {
        tool_use_id: Some(tid.into()),
        ..mk(i, EventKind::ToolResult, json!({
            "tool_result": {
                "tool_use_id": tid,
                "is_error": is_error,
                "content": content
            }
        }))
    }
}

fn tool_call(i: i64, tid: &str, name: &str) -> ObservedEvent {
    ObservedEvent {
        tool_use_id: Some(tid.into()),
        tool_name: Some(name.into()),
        actor: Actor::Assistant,
        kind: EventKind::ToolCall,
        payload: json!({"tool_use_id": tid, "name": name, "input": {}}),
        ..mk(i, EventKind::ToolCall, json!({}))
    }
}

fn otlp_log(i: i64, tid: &str, success: &str) -> ObservedEvent {
    ObservedEvent {
        tool_use_id: Some(tid.into()),
        ..mk(i, EventKind::LogRecord, json!({
            "event_name": "tool_result",
            "attributes": {
                "tool_use_id": tid,
                "success": success
            }
        }))
    }
}

fn view(events: &[ObservedEvent]) -> SessionInsightView<'_> {
    SessionInsightView {
        session_id: "sess_tf",
        events,
        diff_hunks: &[],
        verification_runs: &[],
    }
}

/// KEY REGRESSION: is_error=true with no OTLP/hook → resolve_outcome=Unknown
/// → ToolFailure must NOT fire. Unknown is not a confirmed failure.
#[test]
fn is_error_true_no_otlp_does_not_fire() {
    let events = vec![
        tool_call(0, "tid_1", "Bash"),
        tool_result(1, "tid_1", true, "some error output"),
    ];
    let cands = ToolFailure.detect(&view(&events), &DetectorConfig::default());
    assert!(
        cands.is_empty(),
        "is_error=true without OTLP/hook/exit-code must NOT fire (Unknown outcome); got {} signals",
        cands.len()
    );
}

/// is_error=false + OTLP success=false → resolve_outcome=Failed → fires.
#[test]
fn is_error_false_otlp_failed_fires() {
    let events = vec![
        tool_call(0, "tid_2", "Bash"),
        tool_result(1, "tid_2", false, "build output"),
        otlp_log(2, "tid_2", "false"),
    ];
    let cands = ToolFailure.detect(&view(&events), &DetectorConfig::default());
    assert_eq!(cands.len(), 1, "OTLP success=false must fire ToolFailure");
    // outcome_provenance must be in facts
    assert_eq!(
        cands[0].facts["outcome_provenance"],
        json!("measured"),
        "outcome_provenance must be 'measured' when from OTLP"
    );
    // is_error is kept in facts as tool-execution label
    assert_eq!(
        cands[0].facts["is_error"],
        json!(false),
        "is_error must be preserved in facts (tool-execution indicator)"
    );
}

/// is_error=false + content "exit code: 2" → resolve_outcome=Failed → fires.
#[test]
fn is_error_false_content_exit_code_fires() {
    let events = vec![
        tool_call(0, "tid_3", "Bash"),
        tool_result(1, "tid_3", false, "cargo test failed\nexit code: 2"),
    ];
    let cands = ToolFailure.detect(&view(&events), &DetectorConfig::default());
    assert_eq!(
        cands.len(),
        1,
        "explicit 'exit code: 2' must fire ToolFailure even with is_error=false"
    );
    assert_eq!(cands[0].facts["outcome_provenance"], json!("measured"));
}

/// resolve_outcome=Unknown → ToolFailure does NOT fire (even with is_error=true).
#[test]
fn unknown_outcome_does_not_fire() {
    let events = vec![
        tool_call(0, "tid_4", "Edit"),
        // is_error=true but no OTLP → Unknown
        tool_result(1, "tid_4", true, ""),
    ];
    let cands = ToolFailure.detect(&view(&events), &DetectorConfig::default());
    assert!(
        cands.is_empty(),
        "Unknown outcome must never fire ToolFailure"
    );
}

/// OTLP success=true → resolve_outcome=Passed → no ToolFailure even with is_error=true.
#[test]
fn otlp_passed_does_not_fire() {
    let events = vec![
        tool_call(0, "tid_5", "Bash"),
        tool_result(1, "tid_5", true, ""),
        otlp_log(2, "tid_5", "true"),
    ];
    let cands = ToolFailure.detect(&view(&events), &DetectorConfig::default());
    assert!(
        cands.is_empty(),
        "OTLP success=true (Passed) must not fire ToolFailure even when is_error=true"
    );
}

/// outcome_provenance field is present in facts for OTLP-derived failures.
#[test]
fn facts_contain_outcome_provenance() {
    let events = vec![
        tool_call(0, "tid_6", "Bash"),
        tool_result(1, "tid_6", false, ""),
        otlp_log(2, "tid_6", "false"),
    ];
    let cands = ToolFailure.detect(&view(&events), &DetectorConfig::default());
    assert_eq!(cands.len(), 1);
    assert!(
        cands[0].facts.get("outcome_provenance").is_some(),
        "facts must include outcome_provenance"
    );
}
