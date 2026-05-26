//! Slice-11 — first M5 finding rule (`tool_failure`).
//!
//! The rule must emit a Finding row when an ObservedEvent of kind `tool_result`
//! carries `payload.tool_result.is_error = true`. Evidence points to the
//! graph node that merged the failing tool_result.

#![cfg(test)]

use chrono::Utc;
use serde_json::json;

use witmcc::insight::{rules, run_session_pure};
use witmcc::model::graph::GraphNode;
use witmcc::model::observed::{Actor, EventKind, ObservedEvent};

fn ev_tool_call(event_id: &str, tool_use_id: &str) -> ObservedEvent {
    ObservedEvent {
        event_id: event_id.to_string(),
        raw_event_id: "raw_1".into(),
        schema_version: "0.3.0".into(),
        session_id: "sess_TF".into(),
        observed_at: Utc::now(),
        actor: Actor::Assistant,
        kind: EventKind::ToolCall,
        tool_use_id: Some(tool_use_id.into()),
        tool_name: Some("Bash".into()),
        payload: json!({
            "tool_use": { "id": tool_use_id, "name": "Bash", "input": { "command": "ls /nope" } }
        }),
        ..ObservedEvent::default()
    }
}

fn ev_tool_result_err(event_id: &str, tool_use_id: &str, content: &str) -> ObservedEvent {
    ObservedEvent {
        event_id: event_id.to_string(),
        raw_event_id: "raw_2".into(),
        schema_version: "0.3.0".into(),
        session_id: "sess_TF".into(),
        observed_at: Utc::now(),
        actor: Actor::System,
        kind: EventKind::ToolResult,
        tool_use_id: Some(tool_use_id.into()),
        payload: json!({
            "tool_result": {
                "type": "tool_result",
                "tool_use_id": tool_use_id,
                "content": content,
                "is_error": true
            }
        }),
        ..ObservedEvent::default()
    }
}

fn ev_tool_result_ok(event_id: &str, tool_use_id: &str) -> ObservedEvent {
    ObservedEvent {
        event_id: event_id.to_string(),
        raw_event_id: "raw_3".into(),
        schema_version: "0.3.0".into(),
        session_id: "sess_TF".into(),
        observed_at: Utc::now(),
        actor: Actor::System,
        kind: EventKind::ToolResult,
        tool_use_id: Some(tool_use_id.into()),
        payload: json!({
            "tool_result": { "tool_use_id": tool_use_id, "content": "ok", "is_error": false }
        }),
        ..ObservedEvent::default()
    }
}

fn node_tool_call(node_id: &str, source_event_ids: &[&str]) -> GraphNode {
    GraphNode {
        node_id: node_id.into(),
        schema_version: "0.5.0".into(),
        session_id: "sess_TF".into(),
        node_kind: "tool_call".into(),
        started_at: Utc::now(),
        ended_at: None,
        merge_keys: json!({ "session_id": "sess_TF" }),
        source_event_ids: source_event_ids.iter().map(|s| (*s).into()).collect(),
        source_uris: vec![],
        payload: json!({}),
    }
}

#[test]
fn emits_finding_for_is_error_true_tool_result() {
    let events = vec![
        ev_tool_call("ev_call", "toolu_x"),
        ev_tool_result_err("ev_err", "toolu_x", "command not found"),
    ];
    let nodes = vec![node_tool_call("nd_call", &["ev_call", "ev_err"])];

    let findings = run_session_pure("sess_TF", &events, &nodes);

    assert_eq!(findings.len(), 1, "exactly one tool_failure finding expected");
    let f = &findings[0];
    assert_eq!(f.session_id, "sess_TF");
    assert_eq!(f.category, "tool_failure");
    assert_eq!(f.severity, "medium");
    assert_eq!(f.rule_version, "tool_failure.v1");
    assert!(
        (f.confidence - 0.95).abs() < 1e-6,
        "confidence should be 0.95 (deterministic but pre-subclass)"
    );
    // claim should mention the failure but not be reproduction of tool output
    assert!(
        f.claim.to_lowercase().contains("tool")
            && f.claim.to_lowercase().contains("error"),
        "claim should describe the failure, got: {}",
        f.claim
    );
    // evidence_refs point to the merged tool_call node, role=supporting
    let er = f.evidence_refs.as_array().expect("evidence_refs is array");
    assert_eq!(er.len(), 1);
    let entry = &er[0];
    assert_eq!(entry["node_id"], "nd_call");
    assert_eq!(entry["role"], "supporting");
    // finding_id deterministic: same inputs → same id (idempotent)
    let findings2 = run_session_pure("sess_TF", &events, &nodes);
    assert_eq!(f.finding_id, findings2[0].finding_id);
}

#[test]
fn no_finding_when_all_results_succeeded() {
    let events = vec![
        ev_tool_call("ev_call", "toolu_y"),
        ev_tool_result_ok("ev_ok", "toolu_y"),
    ];
    let nodes = vec![node_tool_call("nd_call", &["ev_call", "ev_ok"])];

    let findings = run_session_pure("sess_TF", &events, &nodes);
    assert!(findings.is_empty(), "no finding expected, got {:?}", findings);
}

#[test]
fn one_finding_per_failed_event_not_per_node() {
    // Two separate tool_calls, both failing → two findings.
    let events = vec![
        ev_tool_call("ev_call_a", "toolu_a"),
        ev_tool_result_err("ev_err_a", "toolu_a", "fail A"),
        ev_tool_call("ev_call_b", "toolu_b"),
        ev_tool_result_err("ev_err_b", "toolu_b", "fail B"),
    ];
    let nodes = vec![
        node_tool_call("nd_a", &["ev_call_a", "ev_err_a"]),
        node_tool_call("nd_b", &["ev_call_b", "ev_err_b"]),
    ];

    let findings = run_session_pure("sess_TF", &events, &nodes);
    assert_eq!(findings.len(), 2);
    let mut node_ids: Vec<&str> = findings
        .iter()
        .map(|f| f.evidence_refs[0]["node_id"].as_str().unwrap())
        .collect();
    node_ids.sort();
    assert_eq!(node_ids, vec!["nd_a", "nd_b"]);
}

#[test]
fn rule_registry_contains_tool_failure() {
    let names: Vec<&str> = rules::all().iter().map(|r| r.name()).collect();
    assert!(
        names.contains(&"tool_failure"),
        "rule registry must include tool_failure, got: {:?}",
        names
    );
}
