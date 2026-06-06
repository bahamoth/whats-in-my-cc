//! Slice-16 — unit tests for the `FinalStateMismatch` L1 extractor.
//! All tests use synthetic `SessionInsightView` data — no DB, no I/O.

use chrono::{TimeZone, Utc};
use serde_json::json;
use wimcc::db::repo_diff_hunk::DiffHunkRow;
use wimcc::db::repo_verification_run::VerificationRunRow;
use wimcc::insight::extractor::InsightExtractor;
use wimcc::insight::extractors::final_state_mismatch::FinalStateMismatch;
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

fn user_message_with_goal(i: usize, text: &str) -> ObservedEvent {
    let mut ev = base_event(i, Actor::User, EventKind::UserMessage);
    ev.payload = json!({ "message": { "content": [{"type":"text","text": text}] } });
    ev
}

fn assistant_message(i: usize, text: &str) -> ObservedEvent {
    let mut ev = base_event(i, Actor::Assistant, EventKind::AssistantMessage);
    ev.payload = json!({ "message": { "content": [{"type":"text","text": text}] } });
    ev
}

fn failed_verification_run(session_id: &str, trigger_event_id: &str) -> VerificationRunRow {
    VerificationRunRow {
        verification_run_id: "vr_001".into(),
        schema_version: "verification_run.v1".into(),
        session_id: session_id.into(),
        source: "bash".into(),
        command: "cargo test".into(),
        command_kind: "test".into(),
        trigger_event_id: trigger_event_id.into(),
        trigger_tool_use_id: None,
        status: "failed".into(),
        detection_basis: "known_tool".into(),
        status_basis: "exit".into(),
        started_at: "2026-01-01T00:00:00Z".into(),
        ended_at: Some("2026-01-01T00:01:00Z".into()),
        exit_code: Some(1),
        failure_summary: Some("2 tests failed".into()),
        raw_event_id: "raw_001".into(),
        parser_version: "v1".into(),
    }
}

fn passed_verification_run(session_id: &str, trigger_event_id: &str) -> VerificationRunRow {
    VerificationRunRow {
        verification_run_id: "vr_001".into(),
        schema_version: "verification_run.v1".into(),
        session_id: session_id.into(),
        source: "bash".into(),
        command: "cargo test".into(),
        command_kind: "test".into(),
        trigger_event_id: trigger_event_id.into(),
        trigger_tool_use_id: None,
        status: "passed".into(),
        detection_basis: "known_tool".into(),
        status_basis: "exit".into(),
        started_at: "2026-01-01T00:00:00Z".into(),
        ended_at: Some("2026-01-01T00:01:00Z".into()),
        exit_code: Some(0),
        failure_summary: None,
        raw_event_id: "raw_001".into(),
        parser_version: "v1".into(),
    }
}

fn view_with_runs<'a>(
    events: &'a [ObservedEvent],
    verification_runs: &'a [VerificationRunRow],
) -> SessionInsightView<'a> {
    SessionInsightView {
        session_id: "sess_t",
        events,
        diff_hunks: &[],
        verification_runs,
    }
}

// ---------------------------------------------------------------------------
// Core firing rule: goal verb + failed final verification + no completion marker
// ---------------------------------------------------------------------------

/// "fix the tests" goal + final verification failed + no completion marker → fires.
#[test]
fn fires_when_goal_unmet_and_no_completion_marker() {
    let events = vec![
        user_message_with_goal(0, "Please fix the failing tests"),
        assistant_message(1, "I'll look into it"),
    ];
    let vr = failed_verification_run("sess_t", "ev_001");
    let runs = vec![vr];
    let view = view_with_runs(&events, &runs);
    let cands = FinalStateMismatch.extract(&view);
    assert_eq!(cands.len(), 1, "fix goal + failed verification must fire");
    let c = &cands[0];
    assert_eq!(c.category, "final_state_mismatch");
    assert!((c.confidence_l1 - 0.6).abs() < f32::EPSILON, "confidence must be 0.6");
    assert_eq!(c.severity, "medium");
}

/// "implement a feature" goal + failed final verification → fires.
#[test]
fn fires_on_implement_goal_with_failed_verification() {
    let events = vec![
        user_message_with_goal(0, "implement the new authentication feature"),
        assistant_message(1, "working on it"),
    ];
    let vr = failed_verification_run("sess_t", "ev_001");
    let runs = vec![vr];
    let view = view_with_runs(&events, &runs);
    let cands = FinalStateMismatch.extract(&view);
    assert_eq!(cands.len(), 1, "implement goal + failed verification must fire");
}

/// Closing verification PASSED → must NOT fire.
#[test]
fn does_not_fire_when_closing_verification_passed() {
    let events = vec![
        user_message_with_goal(0, "fix the tests"),
        assistant_message(1, "tests are now passing"),
    ];
    let vr = passed_verification_run("sess_t", "ev_001");
    let runs = vec![vr];
    let view = view_with_runs(&events, &runs);
    let cands = FinalStateMismatch.extract(&view);
    assert!(cands.is_empty(), "passed verification must not fire final_state_mismatch");
}

/// Final assistant_message contains a completion marker → must NOT fire.
#[test]
fn does_not_fire_when_final_message_has_completion_marker_done() {
    let events = vec![
        user_message_with_goal(0, "fix the tests"),
        assistant_message(1, "All done! The tests are passing now."),
    ];
    // No verification run
    let view = view_with_runs(&events, &[]);
    let cands = FinalStateMismatch.extract(&view);
    assert!(cands.is_empty(), "completion marker 'done' must suppress firing");
}

/// No goal verb in user message → must NOT fire.
#[test]
fn does_not_fire_when_no_goal_verb() {
    let events = vec![
        user_message_with_goal(0, "what is the status of the tests?"),
        assistant_message(1, "some tests are failing"),
    ];
    let vr = failed_verification_run("sess_t", "ev_001");
    let runs = vec![vr];
    let view = view_with_runs(&events, &runs);
    let cands = FinalStateMismatch.extract(&view);
    assert!(cands.is_empty(), "no goal verb must not fire");
}

/// Multiple goals + multiple failures → at most 1 finding (spec §6: session-level grain).
#[test]
fn fires_at_most_once_per_session() {
    let events = vec![
        user_message_with_goal(0, "fix the tests"),
        assistant_message(1, "working"),
        user_message_with_goal(2, "also add a new feature"),
        assistant_message(3, "still working"),
    ];
    let vr1 = failed_verification_run("sess_t", "ev_001");
    let mut vr2 = failed_verification_run("sess_t", "ev_003");
    vr2.verification_run_id = "vr_002".into();
    let runs = vec![vr1, vr2];
    let view = view_with_runs(&events, &runs);
    let cands = FinalStateMismatch.extract(&view);
    assert!(cands.len() <= 1, "at most 1 finding per session, got {}", cands.len());
}

/// Empty session → no fire.
#[test]
fn does_not_fire_on_empty_session() {
    let view = view_with_runs(&[], &[]);
    let cands = FinalStateMismatch.extract(&view);
    assert!(cands.is_empty());
}

// ---------------------------------------------------------------------------
// Evidence projection fields
// ---------------------------------------------------------------------------

#[test]
fn projection_includes_required_fields() {
    let events = vec![
        user_message_with_goal(0, "fix the tests and make them pass"),
        assistant_message(1, "working on it"),
    ];
    let vr = failed_verification_run("sess_t", "ev_001");
    let runs = vec![vr];
    let view = view_with_runs(&events, &runs);
    let cands = FinalStateMismatch.extract(&view);
    assert_eq!(cands.len(), 1);
    let proj = &cands[0].evidence_projection;
    assert_eq!(proj["category"], "final_state_mismatch");
    assert!(proj["goal"]["user_message_event_id"].is_string());
    assert!(proj["goal"]["matched_verbs"].is_array());
    assert!(proj["final_state"]["last_verification_run"]["status"].is_string());
}
