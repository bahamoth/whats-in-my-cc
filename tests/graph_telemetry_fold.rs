//! Slice 1 (Group A) — telemetry fold into owner node payload.
//! Covers tool_result/tool_decision log_record → tool_call by tool_use_id.
//! Folded events MUST NOT remain as standalone nodes and MUST NOT get facet_of edges.
//! (The llm_request span + api_request log → assistant_message fold lands in a
//! follow-up task and is not exercised by this file yet.)

mod common;

use serde_json::{json, Value};
use witmcc::graph::build::compute;
use witmcc::model::observed::{EventKind, ObservedEvent};

fn tool_call_ev(event_id: &str, tuid: &str) -> ObservedEvent {
    let mut e = common::base_event(EventKind::ToolCall, event_id);
    e.tool_use_id = Some(tuid.into());
    e.tool_name = Some("Bash".into());
    e.payload = json!({"tool_name":"Bash","input":{"command":"ls"}});
    e
}

fn tool_result_log_ev(event_id: &str, tuid: &str) -> ObservedEvent {
    let mut e = common::base_event(EventKind::LogRecord, event_id);
    e.payload = json!({
        "event_name":"tool_result",
        "attributes":{"tool_use_id":tuid,"duration_ms":"57","success":"true"}
    });
    e
}

fn tool_decision_log_ev(event_id: &str, tuid: &str) -> ObservedEvent {
    let mut e = common::base_event(EventKind::LogRecord, event_id);
    e.payload = json!({
        "event_name":"tool_decision",
        "attributes":{"tool_use_id":tuid,"decision":"accept","source":"config"}
    });
    e
}

fn facets_of<'a>(nodes: &'a [witmcc::model::graph::GraphNode], kind: &str) -> Vec<&'a Value> {
    let n = nodes.iter().find(|n| n.node_kind == kind).expect("owner node");
    n.payload
        .get("facets")
        .and_then(|f| f.as_array())
        .map(|a| a.iter().collect())
        .unwrap_or_default()
}

#[test]
fn tool_logs_fold_into_tool_call_payload() {
    let evs = vec![
        tool_call_ev("evt-call", "toolu_X"),
        tool_result_log_ev("evt-res", "toolu_X"),
        tool_decision_log_ev("evt-dec", "toolu_X"),
    ];
    let (nodes, edges) = compute("sess_t", &evs, &[], &[]);

    let facets = facets_of(&nodes, "tool_call");
    assert_eq!(facets.len(), 2, "two tool logs folded; got {facets:?}");
    let kinds: Vec<&str> = facets
        .iter()
        .filter_map(|f| f.get("facet_kind").and_then(|v| v.as_str()))
        .collect();
    assert!(kinds.contains(&"tool_result_log"));
    assert!(kinds.contains(&"tool_decision_log"));
    let res = facets.iter().find(|f| f["facet_kind"] == "tool_result_log").unwrap();
    assert_eq!(res["basis"], "tool_use_id");
    assert_eq!(res["source_event_id"], "evt-res");
    assert_eq!(res["data"]["attributes"]["duration_ms"], "57");

    assert!(
        !nodes.iter().any(|n| n.node_kind == "log_record"),
        "folded tool logs must not remain as nodes"
    );
    assert!(
        !edges.iter().any(|e| e.edge_kind == "facet_of"),
        "fold replaces facet_of edges"
    );
}
