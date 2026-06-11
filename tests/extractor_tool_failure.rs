//! Unit tests for the `ToolFailure` detector (Plan 6: outcome-first).
//!
//! The detector fires on `resolve_outcome(...).status == Failed` (not is_error).
//! is_error is kept in facts as a tool-execution indicator.
//! Unknown outcome → no fire.

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

/// OTLP log_record indicating command outcome via success attribute.
fn otlp_success(i: usize, tool_use_id: &str, success: bool) -> ObservedEvent {
    ObservedEvent {
        tool_use_id: Some(tool_use_id.into()),
        payload: json!({
            "event_name": "tool_result",
            "attributes": {
                "tool_use_id": tool_use_id,
                "success": if success { "true" } else { "false" }
            }
        }),
        ..base_event(i, Actor::System, EventKind::LogRecord)
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

/// Plan 6: failure confirmed via OTLP success=false → 1 signal with correct facts.
/// (Previously was triggered by is_error=true; now requires resolve_outcome=Failed.)
#[test]
fn fires_on_otlp_failed_with_no_retry() {
    let events = vec![
        tool_call_ev(0, "tid_0", "Bash"),
        tool_result_ev(1, "tid_0", false), // is_error=false (original bug scenario)
        otlp_success(2, "tid_0", false),   // OTLP confirms failure
    ];
    let view = view_from_events(&events);
    let cands = ToolFailure.detect(&view, &DetectorConfig::default());

    assert_eq!(cands.len(), 1);
    assert_eq!(cands[0].detector, "tool_failure");
    assert!(!cands[0].evidence_refs.is_empty());
    // facts carry is_error (tool-execution indicator) and outcome_provenance.
    assert_eq!(cands[0].facts["is_error"], json!(false));
    assert_eq!(cands[0].facts["outcome_provenance"], json!("measured"));
    assert_eq!(cands[0].facts["tool_name"], json!("Bash"));
    assert_eq!(cands[0].facts["retried"], json!(false));
    // No severity/failure_class judgment.
    assert!(cands[0].facts.get("severity").is_none());
    assert!(cands[0].facts.get("failure_class").is_none());
    assert!(cands[0].subkind.is_none());
}

/// is_error=true alone (no OTLP/hook/exit-code) → Unknown → does NOT fire.
/// This is the key Plan 6 invariant.
#[test]
fn is_error_true_without_otlp_does_not_fire() {
    let events = vec![
        tool_call_ev(0, "tid_0", "Bash"),
        tool_result_ev(1, "tid_0", true), // is_error=true but no OTLP
    ];
    let view = view_from_events(&events);
    let cands = ToolFailure.detect(&view, &DetectorConfig::default());
    assert!(
        cands.is_empty(),
        "is_error=true without OTLP/hook/exit-code → Unknown → must not fire"
    );
}

/// `retried` is a FACT, not a suppression: when a DISTINCT later tool_call
/// re-runs the same operation and Passes, `retried=true` but the original
/// failure signal STILL fires (no suppression).
///
/// A retry is a NEW tool call (new tool_use_id) — NOT the same id. Re-using the
/// same tool_use_id is the SAME invocation, which can never both Fail (the fire
/// condition) and Pass, so retry detection matches a distinct id with the same
/// (tool_name, input).
#[test]
fn retried_is_a_fact_not_a_suppression() {
    let events = vec![
        tool_call_ev(0, "tid_fail", "Bash"),
        tool_result_ev(1, "tid_fail", false),
        otlp_success(2, "tid_fail", false), // failure confirmed by OTLP
        base_filler(3),
        tool_call_ev(4, "tid_retry", "Bash"), // distinct id, same op (input {})
        tool_result_ev(5, "tid_retry", false),
        otlp_success(6, "tid_retry", true), // OTLP confirms the retry Passed
    ];
    let view = view_from_events(&events);
    let cands = ToolFailure.detect(&view, &DetectorConfig::default());
    // The original failure still fires (no suppression); the Passed retry does not.
    assert_eq!(cands.len(), 1, "signal fires even when a retry succeeds");
    assert_eq!(
        cands[0].facts["retried"],
        json!(true),
        "a distinct later call re-running the same op that Passed → retried=true"
    );
}

/// `retried` reflects the configured window.
/// Uses OTLP to confirm failure; no later OTLP success → retried=false.
#[test]
fn retried_window_default_no_retry_window() {
    let mut events = vec![
        tool_call_ev(0, "tid", "Bash"),
        tool_result_ev(1, "tid", false),
        otlp_success(2, "tid", false), // confirms failure
    ];
    for i in 3..9 {
        events.push(base_filler(i));
    }
    // No later success
    let view = view_from_events(&events);
    let default = ToolFailure.detect(&view, &DetectorConfig::default());
    assert_eq!(default.len(), 1);
    // retried should be false since there's no later Passed result
    assert_eq!(default[0].facts["retried"], json!(false));
}

/// tool_result with no `is_error` field and no OTLP → resolve_outcome=Unknown → no fire.
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
    assert!(
        cands.is_empty(),
        "absent is_error + no OTLP → Unknown → no fire"
    );
}

/// Session with OTLP-confirmed passing results → no signals.
#[test]
fn does_not_fire_for_session_with_no_errors() {
    let events = vec![
        tool_call_ev(0, "tid_0", "Read"),
        tool_result_ev(1, "tid_0", false),
        // No OTLP signals → Unknown → no fire
        tool_call_ev(2, "tid_1", "Bash"),
        tool_result_ev(3, "tid_1", false),
    ];
    let view = view_from_events(&events);
    let cands = ToolFailure.detect(&view, &DetectorConfig::default());
    assert!(cands.is_empty());
}

/// Two distinct OTLP-confirmed tool failures → two signals.
#[test]
fn fires_for_each_distinct_tool_failure() {
    let events = vec![
        tool_call_ev(0, "tid_0", "Bash"),
        tool_result_ev(1, "tid_0", false),
        otlp_success(2, "tid_0", false),
        tool_call_ev(3, "tid_1", "Bash"),
        tool_result_ev(4, "tid_1", false),
        otlp_success(5, "tid_1", false),
    ];
    let view = view_from_events(&events);
    let cands = ToolFailure.detect(&view, &DetectorConfig::default());
    assert_eq!(cands.len(), 2);
}

/// tool_name is taken from the paired call event.
#[test]
fn tool_name_comes_from_paired_call() {
    let events = vec![
        tool_call_ev(0, "tid_0", "Edit"),
        tool_result_ev(1, "tid_0", false),
        otlp_success(2, "tid_0", false),
    ];
    let view = view_from_events(&events);
    let cands = ToolFailure.detect(&view, &DetectorConfig::default());
    assert_eq!(cands.len(), 1);
    assert_eq!(cands[0].facts["tool_name"], json!("Edit"));
    assert!(cands[0].summary.contains("Edit"));
}

/// The error content is exposed verbatim as a fact (no benign/internal labeling).
/// Uses OTLP to confirm failure (is_error alone not sufficient).
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
                    "is_error": false,
                    "content": "grep: no matches found"
                }
            }),
            ..base_event(1, Actor::Tool, EventKind::ToolResult)
        },
        otlp_success(2, "tid_0", false),
    ];
    let view = view_from_events(&events);
    let cands = ToolFailure.detect(&view, &DetectorConfig::default());
    assert_eq!(cands.len(), 1);
    // Raw excerpt, NOT classified as benign_nonzero_exit (assumption removed).
    assert_eq!(
        cands[0].facts["error_excerpt"],
        json!("grep: no matches found")
    );
    assert!(cands[0].facts.get("failure_class").is_none());
}

/// `<tool_use_error>` 래퍼(하니스 구조화 에러 채널) → resolve_outcome=Failed(Measured)
/// → 비-Bash 도구(Edit)의 실패도 발화한다. 코퍼스 ~790건(stale-read 431 등)이
/// 이 경로로만 관측 가능 — real fixture: disposition_v01.jsonl (session 5864d6c7).
#[test]
fn fires_on_edit_tool_use_error_without_exit_code() {
    let mut result = tool_result_ev(1, "tid_e", true);
    result.payload = json!({
        "content_ordinal": 0,
        "tool_result": {
            "type": "tool_result",
            "tool_use_id": "tid_e",
            "is_error": true,
            "content": "<tool_use_error>File has been modified since read, either by the user or by a linter.</tool_use_error>"
        }
    });
    let events = vec![tool_call_ev(0, "tid_e", "Edit"), result];
    let view = view_from_events(&events);
    let cands = ToolFailure.detect(&view, &DetectorConfig::default());

    assert_eq!(cands.len(), 1);
    assert_eq!(cands[0].detector, "tool_failure");
}

/// 병렬 호출 취소는 실행 실패가 아니다 → 발화하지 않는다.
#[test]
fn does_not_fire_on_cancelled_parallel_call() {
    let mut result = tool_result_ev(1, "tid_c", true);
    result.payload = json!({
        "content_ordinal": 0,
        "tool_result": {
            "type": "tool_result",
            "tool_use_id": "tid_c",
            "is_error": true,
            "content": "<tool_use_error>Cancelled: parallel tool call Bash(x)</tool_use_error>"
        }
    });
    let events = vec![tool_call_ev(0, "tid_c", "Bash"), result];
    let view = view_from_events(&events);
    let cands = ToolFailure.detect(&view, &DetectorConfig::default());

    assert!(cands.is_empty());
}
