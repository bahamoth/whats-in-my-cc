//! Slice 1 (Group A) — telemetry fold into owner node payload.
//! Covers both folds:
//!   - tool_result/tool_decision log_record → tool_call by tool_use_id
//!   - llm_request otel_span + api_request log_record → assistant_message by request_id
//! Folded events MUST NOT remain as standalone nodes and MUST NOT get facet_of edges.

mod common;

use serde_json::{json, Value};
use wimcc::graph::build::compute;
use wimcc::model::observed::{EventKind, ObservedEvent};

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

fn facets_of<'a>(nodes: &'a [wimcc::model::graph::GraphNode], kind: &str) -> Vec<&'a Value> {
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

fn assistant_ev(event_id: &str, rid: &str) -> ObservedEvent {
    let mut e = common::base_event(EventKind::AssistantMessage, event_id);
    e.request_id = Some(rid.into());
    e.payload = json!({"role":"assistant","content":[]});
    e
}

fn llm_span_ev(event_id: &str, rid: &str) -> ObservedEvent {
    let mut e = common::base_event(EventKind::OtelSpan, event_id);
    e.trace_id = Some("trace-1".into());
    e.span_id = Some("span-1".into());
    e.payload = json!({
        "raw_span":{
            "name":"claude_code.llm_request",
            "attributes":[
                {"key":"request_id","value":{"stringValue":rid}},
                {"key":"duration_ms","value":{"stringValue":"1521"}}
            ]
        }
    });
    e
}

fn api_request_log_ev(event_id: &str, rid: &str) -> ObservedEvent {
    let mut e = common::base_event(EventKind::LogRecord, event_id);
    e.payload = json!({
        "event_name":"api_request",
        "attributes":{"request_id":rid,"cost_usd":0.000906,"duration_ms":"1521","model":"claude-haiku-4-5-20251001"}
    });
    e
}

#[test]
fn span_and_api_log_fold_into_assistant_payload() {
    let evs = vec![
        assistant_ev("evt-asst", "req_A"),
        llm_span_ev("evt-span", "req_A"),
        api_request_log_ev("evt-api", "req_A"),
    ];
    let (nodes, edges) = compute("sess_t", &evs, &[], &[]);

    let facets = facets_of(&nodes, "assistant_message");
    let kinds: Vec<&str> = facets
        .iter()
        .filter_map(|f| f.get("facet_kind").and_then(|v| v.as_str()))
        .collect();
    assert!(kinds.contains(&"llm_request_span"), "span folded; got {kinds:?}");
    assert!(kinds.contains(&"api_request_log"), "api log folded; got {kinds:?}");

    let api = facets.iter().find(|f| f["facet_kind"] == "api_request_log").unwrap();
    assert_eq!(api["data"]["attributes"]["cost_usd"], 0.000906);

    assert!(!nodes.iter().any(|n| n.node_kind == "otel_span"));
    assert!(!nodes.iter().any(|n| n.node_kind == "log_record"));
    assert!(!edges.iter().any(|e| e.edge_kind == "facet_of"));
}
