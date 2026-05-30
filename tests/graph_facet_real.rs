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

    let kind_by_id: std::collections::HashMap<_, _> =
        nodes.iter().map(|n| (n.node_id.as_str(), n.node_kind.as_str())).collect();
    let facets: Vec<_> = edges.iter().filter(|e| e.edge_kind == "facet_of").collect();

    // 2 logs → tool_call, 1 span → assistant_message
    assert_eq!(facets.len(), 3, "expected 3 facet_of edges from real payloads, got {}", facets.len());
    for f in &facets {
        let to_kind = kind_by_id.get(f.to_node_id.as_str()).copied().unwrap_or("");
        assert!(
            matches!(to_kind, "tool_call" | "assistant_message"),
            "facet_of target must be an entity node, got kind={to_kind}"
        );
    }
    // basis values present and correct
    let bases: std::collections::HashSet<_> = facets.iter()
        .filter_map(|f| f.attributes.get("basis").and_then(|v| v.as_str())).collect();
    assert!(bases.contains("tool_use_id"), "tool_use_id basis present");
    assert!(bases.contains("request_id"), "request_id basis present");
}
