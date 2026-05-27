//! Slice-14 — unit tests for the `ToolFailure` L1 extractor.
//! All tests use synthetic `SessionInsightView` data — no DB, no I/O.

use chrono::{TimeZone, Utc};
use serde_json::json;
use witmcc::insight::extractors::tool_failure::ToolFailure;
use witmcc::insight::extractor::InsightExtractor;
use witmcc::insight::view::SessionInsightView;
use witmcc::model::observed::{Actor, EventKind, ObservedEvent};

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

fn tool_call_ev(i: usize, tool_use_id: &str, tool_name: &str) -> ObservedEvent {
    ObservedEvent {
        tool_use_id: Some(tool_use_id.into()),
        tool_name: Some(tool_name.into()),
        payload: json!({ "tool_use_id": tool_use_id, "name": tool_name, "input": {} }),
        ..base_event(i, Actor::Assistant, EventKind::ToolCall)
    }
}

fn tool_result_ev(i: usize, tool_use_id: &str, is_error: bool) -> ObservedEvent {
    ObservedEvent {
        tool_use_id: Some(tool_use_id.into()),
        payload: json!({
            "tool_use_id": tool_use_id,
            "is_error": is_error,
            "content": if is_error { "error output" } else { "ok" }
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
        episodes: &[],
        nodes: &[],
        edges: &[],
    }
}

/// One is_error=true result with no retry within 5 events → 1 candidate.
#[test]
fn fires_on_is_error_true_with_no_retry() {
    let events = vec![
        tool_call_ev(0, "tid_0", "Bash"),
        tool_result_ev(1, "tid_0", true),
    ];
    let view = view_from_events(&events);
    let cands = ToolFailure.extract(&view);

    assert_eq!(cands.len(), 1, "expected 1 candidate, got {:?}", cands.len());
    assert_eq!(cands[0].category, "tool_failure");
    assert!(
        (cands[0].confidence_l1 - 1.0).abs() < f32::EPSILON,
        "confidence_l1 must be 1.0, got {}",
        cands[0].confidence_l1
    );
    assert!(
        !cands[0].evidence_refs.is_empty(),
        "evidence_refs must be non-empty"
    );
}

/// tool_result is_error=true, then within 5 events a successful result for the
/// same tool_use_id → rule must NOT fire (retry succeeded).
#[test]
fn does_not_fire_if_same_tool_use_succeeds_within_5_events() {
    let events = vec![
        tool_call_ev(0, "tid_0", "Bash"),
        tool_result_ev(1, "tid_0", true),
        // some other event
        base_event(2, Actor::Assistant, EventKind::AssistantMessage),
        // successful retry for same tool_use_id within 5 events
        tool_result_ev(3, "tid_0", false),
    ];
    let view = view_from_events(&events);
    let cands = ToolFailure.extract(&view);

    assert!(
        cands.is_empty(),
        "must not fire when same tool_use_id succeeds within 5 events, got {:?}",
        cands.len()
    );
}

/// tool_result with no `is_error` field at all → treat as false, no fire.
#[test]
fn does_not_fire_if_no_is_error_field() {
    let events = vec![
        tool_call_ev(0, "tid_0", "Bash"),
        ObservedEvent {
            tool_use_id: Some("tid_0".into()),
            payload: json!({ "tool_use_id": "tid_0", "content": "ok" }),
            ..base_event(1, Actor::Tool, EventKind::ToolResult)
        },
    ];
    let view = view_from_events(&events);
    let cands = ToolFailure.extract(&view);

    assert!(
        cands.is_empty(),
        "must not fire when is_error field absent (treat as false), got {:?}",
        cands.len()
    );
}

/// Session with zero tool results → no candidates.
#[test]
fn does_not_fire_for_session_with_no_errors() {
    let events = vec![
        tool_call_ev(0, "tid_0", "Read"),
        tool_result_ev(1, "tid_0", false),
        tool_call_ev(2, "tid_1", "Bash"),
        tool_result_ev(3, "tid_1", false),
    ];
    let view = view_from_events(&events);
    let cands = ToolFailure.extract(&view);

    assert!(
        cands.is_empty(),
        "must not fire when all tools succeed, got {:?}",
        cands.len()
    );
}

/// Two distinct tool failures (different tool_use_ids) → two candidates.
#[test]
fn fires_for_each_distinct_tool_failure() {
    let events = vec![
        tool_call_ev(0, "tid_0", "Bash"),
        tool_result_ev(1, "tid_0", true),
        tool_call_ev(2, "tid_1", "Bash"),
        tool_result_ev(3, "tid_1", true),
    ];
    let view = view_from_events(&events);
    let cands = ToolFailure.extract(&view);

    assert_eq!(
        cands.len(),
        2,
        "two distinct failures should produce two candidates, got {:?}",
        cands.len()
    );
}

/// Retry success comes too late (>5 events away) → rule fires.
#[test]
fn fires_when_retry_success_is_beyond_5_events() {
    let mut events = vec![
        tool_call_ev(0, "tid_0", "Bash"),
        tool_result_ev(1, "tid_0", true),
    ];
    // pad 5 unrelated events
    for i in 2..8 {
        events.push(base_event(i, Actor::Assistant, EventKind::AssistantMessage));
    }
    // success outside window
    events.push(tool_result_ev(8, "tid_0", false));
    let view = view_from_events(&events);
    let cands = ToolFailure.extract(&view);

    assert_eq!(
        cands.len(),
        1,
        "must fire when success retry is beyond 5 events, got {:?}",
        cands.len()
    );
}
