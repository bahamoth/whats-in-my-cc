mod common;
use witmcc::graph::build::compute;
use witmcc::model::observed::{EventKind, ObservedEvent};
use serde_json::json;

fn tool_call_ev(event_id: &str, tuid: &str) -> ObservedEvent {
    let mut e = common::base_event(EventKind::ToolCall, event_id);
    e.tool_use_id = Some(tuid.into());
    e.tool_name = Some("Bash".into());
    e.payload = json!({"tool_name":"Bash","input":{"command":"ls"}});
    e
}
fn tool_log_ev(event_id: &str, tuid: &str) -> ObservedEvent {
    let mut e = common::base_event(EventKind::LogRecord, event_id);
    e.payload = json!({
        "event_name":"tool_result",
        "attributes":{"tool_use_id":tuid,"duration_ms":"57","success":"true"}
    });
    e
}
fn assistant_ev(event_id: &str, rid: &str) -> ObservedEvent {
    let mut e = common::base_event(EventKind::AssistantMessage, event_id);
    e.request_id = Some(rid.into());
    e.payload = json!({"text":"hi","model":"claude-opus-4-8"});
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
                {"key":"duration_ms","value":{"stringValue":"28900"}}
            ]
        }
    });
    e
}
// A non-llm_request span (e.g. a tool span) that happens to carry a request_id
// attribute. The name filter must exclude it from facet_of wiring.
fn tool_span_ev(event_id: &str, rid: &str) -> ObservedEvent {
    let mut e = common::base_event(EventKind::OtelSpan, event_id);
    e.trace_id = Some("trace-2".into());
    e.span_id = Some("span-2".into());
    e.payload = json!({
        "raw_span":{
            "name":"claude_code.tool",
            "attributes":[
                {"key":"request_id","value":{"stringValue":rid}},
                {"key":"duration_ms","value":{"stringValue":"57"}}
            ]
        }
    });
    e
}

// Slice 1 (Group A): tool_result/tool_decision log_record events fold INTO the
// owning tool_call node's payload.facets (keyed by tool_use_id) instead of being
// promoted to standalone nodes linked by a facet_of edge. The folded log must not
// remain as a node and must not produce a facet_of edge.
#[test]
fn tool_log_folds_into_tool_call_payload_no_facet_of_edge() {
    let evs = vec![tool_call_ev("evt-call", "toolu_X"), tool_log_ev("evt-log", "toolu_X")];
    let (nodes, edges) = compute("sess_t", &evs, &[], &[]);
    let call = nodes.iter().find(|n| n.node_kind == "tool_call").unwrap();
    let facets = call
        .payload
        .get("facets")
        .and_then(|f| f.as_array())
        .expect("tool_call has payload.facets");
    assert_eq!(facets.len(), 1, "one tool log folded");
    assert_eq!(facets[0]["facet_kind"], "tool_result_log");
    assert_eq!(facets[0]["basis"], "tool_use_id");
    assert!(
        !nodes.iter().any(|n| n.node_kind == "log_record"),
        "folded tool log must not remain as a node"
    );
    assert!(
        !edges.iter().any(|e| e.edge_kind == "facet_of"),
        "fold replaces the tool-log facet_of edge"
    );
}

// An orphan tool log (no matching tool_call) cannot be folded; it stays a
// standalone log_record node and still produces no facet_of edge.
#[test]
fn facet_of_not_emitted_when_no_matching_tool_call() {
    let evs = vec![tool_log_ev("evt-log-orphan", "toolu_orphan")];
    let (nodes, edges) = compute("sess_t", &evs, &[], &[]);
    assert!(edges.iter().all(|e| e.edge_kind != "facet_of"));
    assert!(
        nodes.iter().any(|n| n.node_kind == "log_record"),
        "unfolded orphan log remains a standalone node"
    );
}

#[test]
fn facet_of_links_llm_span_to_assistant_by_request_id() {
    let evs = vec![assistant_ev("evt-asst", "req_A"), llm_span_ev("evt-span", "req_A")];
    let (nodes, edges) = compute("sess_t", &evs, &[], &[]);
    let asst = nodes.iter().find(|n| n.node_kind == "assistant_message").unwrap();
    let span = nodes.iter().find(|n| n.node_kind == "otel_span").unwrap();
    let f = edges.iter().find(|e| e.edge_kind == "facet_of"
        && e.from_node_id == span.node_id).expect("span→asst facet_of");
    assert_eq!(f.to_node_id, asst.node_id);
    assert_eq!(f.attributes.get("basis").and_then(|v| v.as_str()), Some("request_id"));
}

#[test]
fn facet_of_not_emitted_for_non_llm_request_span_with_request_id() {
    let evs = vec![assistant_ev("evt-asst", "req_A"), tool_span_ev("evt-tool-span", "req_A")];
    let (nodes, edges) = compute("sess_t", &evs, &[], &[]);
    let span = nodes.iter().find(|n| n.node_kind == "otel_span").unwrap();
    assert!(
        edges.iter().all(|e| !(e.edge_kind == "facet_of" && e.from_node_id == span.node_id)),
        "non-llm_request span must not produce a facet_of edge"
    );
}
