use chrono::{TimeZone, Utc};
use witmcc::insight::episode::classifier::classify_session;
use witmcc::insight::episode::types::Phase;
use witmcc::model::observed::{Actor, EventKind, ObservedEvent};

fn ev(i: usize, actor: Actor, kind: EventKind, tool: Option<&str>) -> ObservedEvent {
    ObservedEvent {
        event_id: format!("ev_{i:03}"),
        raw_event_id: format!("raw_{i:03}"),
        schema_version: "observed_event.v1".into(),
        session_id: "sess_t".into(),
        event_uuid: Some(format!("uuid_{i}")),
        observed_at: Utc.timestamp_opt(1_700_000_000 + i as i64, 0).unwrap(),
        actor,
        kind,
        tool_name: tool.map(String::from),
        parser_version: "test".into(),
        ..Default::default()
    }
}

#[test]
fn classifies_intake_then_exploration_then_action() {
    let evs = vec![
        ev(0, Actor::User, EventKind::UserMessage, None),
        ev(1, Actor::Assistant, EventKind::ToolCall, Some("Read")),
        ev(2, Actor::Tool, EventKind::ToolResult, Some("Read")),
        ev(3, Actor::Assistant, EventKind::ToolCall, Some("Edit")),
        ev(4, Actor::Tool, EventKind::ToolResult, Some("Edit")),
    ];
    let eps = classify_session("sess_t", &evs, &[]);
    let phases: Vec<Phase> = eps.iter().map(|e| e.phase).collect();
    assert_eq!(phases, vec![Phase::Intake, Phase::Exploration, Phase::Action]);
}

#[test]
fn empty_session_emits_zero_episodes() {
    let eps = classify_session("sess_t", &[], &[]);
    assert!(eps.is_empty());
}

#[test]
fn diagnosis_after_error() {
    let evs = vec![
        ev(0, Actor::User, EventKind::UserMessage, None),
        ev(1, Actor::Assistant, EventKind::ToolCall, Some("Bash")),
        {
            let mut e = ev(2, Actor::Tool, EventKind::ToolResult, Some("Bash"));
            e.payload = serde_json::json!({"is_error": true, "stderr": "boom"});
            e
        },
        ev(3, Actor::Assistant, EventKind::ToolCall, Some("Read")),
        ev(4, Actor::Tool, EventKind::ToolResult, Some("Read")),
    ];
    let eps = classify_session("sess_t", &evs, &[]);
    assert!(
        eps.iter().any(|e| e.phase == Phase::Diagnosis),
        "expected diagnosis phase; got {:?}",
        eps.iter().map(|e| e.phase).collect::<Vec<_>>()
    );
}

#[test]
fn verification_phase_when_run_present() {
    use witmcc::db::repo_verification_run::VerificationRunRow;
    let evs = vec![
        ev(0, Actor::User, EventKind::UserMessage, None),
        ev(1, Actor::Assistant, EventKind::ToolCall, Some("Bash")),
        ev(2, Actor::Tool, EventKind::ToolResult, Some("Bash")),
    ];
    let runs = vec![VerificationRunRow {
        trigger_event_id: "ev_002".into(),
        started_at: "1970-01-01T00:00:00Z".into(),
        ..Default::default()
    }];
    let eps = classify_session("sess_t", &evs, &runs);
    assert!(eps.iter().any(|e| e.phase == Phase::Verification));
}
