mod common;
use witmcc::graph::build::compute;
use witmcc::model::observed::{EventKind, ObservedEvent};
use serde_json::{json, Value};

fn load_fixture() -> Value {
    let raw = std::fs::read_to_string("tests/fixtures/facet/real/facet_correlation_v01.json")
        .expect("facet fixture must exist");
    serde_json::from_str(&raw).unwrap()
}

#[test]
fn real_payloads_produce_facet_of_with_valid_entity_targets() {
    let fx = load_fixture();
    let tuid = fx["tool_use_id"].as_str().unwrap();
    let rid = fx["request_id"].as_str().unwrap();

    let mut tool_call = common::base_event(EventKind::ToolCall, "evt-call");
    tool_call.tool_use_id = Some(tuid.into());
    tool_call.tool_name = Some("Bash".into());
    tool_call.payload = json!({"tool_name":"Bash","input":{"command":"git branch"}});

    let mut log_result = common::base_event(EventKind::LogRecord, "evt-log-result");
    log_result.payload = fx["log_tool_result_payload"].clone();

    let mut log_decision = common::base_event(EventKind::LogRecord, "evt-log-decision");
    log_decision.payload = fx["log_tool_decision_payload"].clone();

    let mut assistant = common::base_event(EventKind::AssistantMessage, "evt-asst");
    assistant.request_id = Some(rid.into());
    assistant.payload = json!({"text":"...", "model":"claude-opus-4-8"});

    let mut span = common::base_event(EventKind::OtelSpan, "evt-span");
    span.trace_id = Some("edc17079c07f112810437d517bcfe709".into());
    span.span_id = Some("0093d9c7b4d18aeb".into());
    span.payload = fx["llm_request_span_payload"].clone();

    let evs = vec![tool_call, log_result, log_decision, assistant, span];
    let (nodes, edges) = compute("sess-real-facet", &evs, &[], &[]);

    // Slice 1 (Group A): every Group-A telemetry kind now folds INTO its owner
    // node's payload.facets — the 2 tool logs (tool_result + tool_decision) into
    // the tool_call (tool_use_id), and the llm_request span into the
    // assistant_message (request_id). No facet_of edges remain.
    assert!(
        !edges.iter().any(|e| e.edge_kind == "facet_of"),
        "no facet_of edges remain after the fold"
    );

    // The llm_request span folds into the assistant_message payload, not left as
    // a node nor linked by a facet_of edge.
    let asst = nodes
        .iter()
        .find(|n| n.node_kind == "assistant_message")
        .expect("assistant_message node");
    let asst_facets = asst
        .payload
        .get("facets")
        .and_then(|f| f.as_array())
        .expect("assistant_message has payload.facets");
    let asst_fkinds: std::collections::HashSet<_> = asst_facets
        .iter()
        .filter_map(|f| f.get("facet_kind").and_then(|v| v.as_str()))
        .collect();
    assert!(asst_fkinds.contains("llm_request_span"), "llm_request span folded");
    let span_facet = asst_facets
        .iter()
        .find(|f| f["facet_kind"] == "llm_request_span")
        .unwrap();
    assert_eq!(span_facet["basis"], "request_id");
    assert!(
        !nodes.iter().any(|n| n.node_kind == "otel_span"),
        "folded llm_request span must not remain as a standalone node"
    );

    // The two tool logs are folded into the tool_call payload, not left as nodes.
    let call = nodes.iter().find(|n| n.node_kind == "tool_call").expect("tool_call node");
    let payload_facets = call
        .payload
        .get("facets")
        .and_then(|f| f.as_array())
        .expect("tool_call has payload.facets");
    assert_eq!(payload_facets.len(), 2, "both real tool logs folded");
    let fkinds: std::collections::HashSet<_> = payload_facets
        .iter()
        .filter_map(|f| f.get("facet_kind").and_then(|v| v.as_str()))
        .collect();
    assert!(fkinds.contains("tool_result_log"), "tool_result_log folded");
    assert!(fkinds.contains("tool_decision_log"), "tool_decision_log folded");
    assert!(
        !nodes.iter().any(|n| n.node_kind == "log_record"),
        "folded tool logs must not remain as standalone nodes"
    );
}
