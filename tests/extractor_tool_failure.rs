//! Unit tests for the `ToolFailure` detector (Plan 1: finding → signal).
//! All tests use synthetic `SessionInsightView` data — no DB, no I/O.
//!
//! The detector emits FACTS only — `is_error` / `retried` / `tool_name` /
//! `error_excerpt`. NO severity/failure-class/benign/internal judgment (the 3
//! removed assumptions). Those classifications are now LLM/human work (§6.3).

use chrono::{TimeZone, Utc};
use serde_json::json;
use wimcc::insight::config::DetectorConfig;
use wimcc::insight::extractor::Detector;
use wimcc::insight::extractors::tool_failure::ToolFailure;
use wimcc::insight::view::SessionInsightView;
use wimcc::model::observed::{Actor, EventKind, ObservedEvent};

fn base_event(i: usize, actor: Actor, kind: EventKind) -> ObservedEvent {
    ObservedEvent {
        event_id: format!("ev_{i:03}"),
        raw_event_id: format!("raw_{i:03}"),
        schema_version: "observed_event.v1".into(),
        session_id: "sess_t".into(),
        event_uuid: Some(format!("uuid_{i}")),
        observed_at: Utc.timestamp_opt(1_700_000_000 + i as i64 * 10, 0).unwrap(),
        actor,
        kind,
        parser_version: "test".into(),
        ..Default::default()
    }
}

fn base_filler(i: usize) -> ObservedEvent {
    base_event(i, Actor::Assistant, EventKind::AssistantMessage)
}

fn tool_call_ev(i: usize, tool_use_id: &str, tool_name: &str) -> ObservedEvent {
    ObservedEvent {
        tool_use_id: Some(tool_use_id.into()),
        tool_name: Some(tool_name.into()),
        payload: json!({ "tool_use_id": tool_use_id, "name": tool_name, "input": {} }),
        ..base_event(i, Actor::Assistant, EventKind::ToolCall)
    }
}

/// Build a tool_result event using the real transcript payload shape:
/// `{"content_ordinal": 0, "tool_result": {"tool_use_id": …, "is_error": …, "content": …}}`.
/// Verified against real fixture: aac68973-729e-4014-a02b-28a556f5ff29.
fn tool_result_ev(i: usize, tool_use_id: &str, is_error: bool) -> ObservedEvent {
    ObservedEvent {
        tool_use_id: Some(tool_use_id.into()),
        payload: json!({
            "content_ordinal": 0,
            "tool_result": {
                "type": "tool_result",
                "tool_use_id": tool_use_id,
                "is_error": is_error,
                "content": if is_error { "error output" } else { "ok" }
            }
        }),
        ..base_event(i, Actor::Tool, EventKind::ToolResult)
    }
}

fn view_from_events(events: &[ObservedEvent]) -> SessionInsightView<'_> {
    SessionInsightView {
        session_id: "sess_t",
        events,
        diff_hunks: &[],
        verification_runs: &[],
    }
}

/// One is_error=true result with no retry → 1 signal carrying raw facts.
#[test]
fn fires_on_is_error_true_with_no_retry() {
    let events = vec![
        tool_call_ev(0, "tid_0", "Bash"),
        tool_result_ev(1, "tid_0", true),
    ];
    let view = view_from_events(&events);
    let cands = ToolFailure.detect(&view, &DetectorConfig::default());

    assert_eq!(cands.len(), 1);
    assert_eq!(cands[0].detector, "tool_failure");
    assert!(!cands[0].evidence_refs.is_empty());
    // facts carry raw is_error + tool_name; NO severity/class judgment.
    assert_eq!(cands[0].facts["is_error"], json!(true));
    assert_eq!(cands[0].facts["tool_name"], json!("Bash"));
    assert_eq!(cands[0].facts["retried"], json!(false));
    // The 3 removed assumptions leave no trace: no severity / failure_class.
    assert!(cands[0].facts.get("severity").is_none());
    assert!(cands[0].facts.get("failure_class").is_none());
    assert!(cands[0].subkind.is_none());
}

/// `retried` is a FACT: a later success for the same tool_use_id within the
/// window sets `retried=true` but the signal STILL fires (no suppression).
#[test]
fn retried_is_a_fact_not_a_suppression() {
    let events = vec![
        tool_call_ev(0, "tid_0", "Bash"),
        tool_result_ev(1, "tid_0", true),
        base_filler(2),
        tool_result_ev(3, "tid_0", false),
    ];
    let view = view_from_events(&events);
    let cands = ToolFailure.detect(&view, &DetectorConfig::default());
    assert_eq!(cands.len(), 1, "retry success is a fact, not a suppression");
    assert_eq!(cands[0].facts["retried"], json!(true));
}

/// `retried` reflects the configured window: a success at distance 7 is outside
/// the default window (5) → retried=false; a window of 10 captures it → true.
#[test]
fn retried_window_from_config() {
    let mut events = vec![
        tool_call_ev(0, "tid", "Bash"),
        tool_result_ev(1, "tid", true),
    ];
    for i in 2..8 {
        events.push(base_filler(i));
    }
    events.push(tool_result_ev(8, "tid", false)); // success retry, distance 7
    let view = view_from_events(&events);

    let default = ToolFailure.detect(&view, &DetectorConfig::default());
    assert_eq!(default.len(), 1);
    assert_eq!(default[0].facts["retried"], json!(false), "distance 7 > window 5");

    let cfg = DetectorConfig::from_toml_str("[detector.tool_failure]\nretry_window = 10\n");
    let widened = ToolFailure.detect(&view, &cfg);
    assert_eq!(widened.len(), 1);
    assert_eq!(widened[0].facts["retried"], json!(true), "window 10 captures distance 7");
}

/// tool_result with no `is_error` field at all → treat as false, no fire.
#[test]
fn does_not_fire_if_no_is_error_field() {
    let events = vec![
        tool_call_ev(0, "tid_0", "Bash"),
        ObservedEvent {
            tool_use_id: Some("tid_0".into()),
            payload: json!({ "content_ordinal": 0, "tool_result": { "tool_use_id": "tid_0", "content": "ok" } }),
            ..base_event(1, Actor::Tool, EventKind::ToolResult)
        },
    ];
    let view = view_from_events(&events);
    let cands = ToolFailure.detect(&view, &DetectorConfig::default());
    assert!(cands.is_empty(), "absent is_error must be treated as false");
}

/// Session with zero tool errors → no signals.
#[test]
fn does_not_fire_for_session_with_no_errors() {
    let events = vec![
        tool_call_ev(0, "tid_0", "Read"),
        tool_result_ev(1, "tid_0", false),
        tool_call_ev(2, "tid_1", "Bash"),
        tool_result_ev(3, "tid_1", false),
    ];
    let view = view_from_events(&events);
    let cands = ToolFailure.detect(&view, &DetectorConfig::default());
    assert!(cands.is_empty());
}

/// Two distinct tool failures (different tool_use_ids) → two signals.
#[test]
fn fires_for_each_distinct_tool_failure() {
    let events = vec![
        tool_call_ev(0, "tid_0", "Bash"),
        tool_result_ev(1, "tid_0", true),
        tool_call_ev(2, "tid_1", "Bash"),
        tool_result_ev(3, "tid_1", true),
    ];
    let view = view_from_events(&events);
    let cands = ToolFailure.detect(&view, &DetectorConfig::default());
    assert_eq!(cands.len(), 2);
}

/// tool_name is taken from the paired call event (tool_result has no tool_name).
#[test]
fn tool_name_comes_from_paired_call() {
    let events = vec![
        tool_call_ev(0, "tid_0", "Edit"),
        tool_result_ev(1, "tid_0", true),
    ];
    let view = view_from_events(&events);
    let cands = ToolFailure.detect(&view, &DetectorConfig::default());
    assert_eq!(cands.len(), 1);
    assert_eq!(cands[0].facts["tool_name"], json!("Edit"));
    assert!(cands[0].summary.contains("Edit"));
}

/// The error content is exposed verbatim as a fact (no benign/internal labeling).
#[test]
fn error_excerpt_is_raw_fact() {
    let events = vec![
        tool_call_ev(0, "tid_0", "Bash"),
        ObservedEvent {
            tool_use_id: Some("tid_0".into()),
            payload: json!({
                "content_ordinal": 0,
                "tool_result": {
                    "tool_use_id": "tid_0",
                    "is_error": true,
                    "content": "grep: no matches found"
                }
            }),
            ..base_event(1, Actor::Tool, EventKind::ToolResult)
        },
    ];
    let view = view_from_events(&events);
    let cands = ToolFailure.detect(&view, &DetectorConfig::default());
    assert_eq!(cands.len(), 1);
    // Raw excerpt, NOT classified as benign_nonzero_exit (assumption removed).
    assert_eq!(cands[0].facts["error_excerpt"], json!("grep: no matches found"));
    assert!(cands[0].facts.get("failure_class").is_none());
}
