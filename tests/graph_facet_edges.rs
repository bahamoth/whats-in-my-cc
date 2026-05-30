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

#[test]
fn facet_of_links_log_record_to_tool_call_by_tool_use_id() {
    let evs = vec![tool_call_ev("evt-call", "toolu_X"), tool_log_ev("evt-log", "toolu_X")];
    let (nodes, edges) = compute("sess_t", &evs, &[], &[]);
    let call = nodes.iter().find(|n| n.node_kind == "tool_call").unwrap();
    let log = nodes.iter().find(|n| n.node_kind == "log_record").unwrap();
    let f: Vec<_> = edges.iter().filter(|e| e.edge_kind == "facet_of").collect();
    assert_eq!(f.len(), 1, "정확히 하나의 facet_of");
    assert_eq!(f[0].from_node_id, log.node_id, "from=facet(log)");
    assert_eq!(f[0].to_node_id, call.node_id, "to=엔티티(tool_call)");
    assert_eq!(f[0].attributes.get("basis").and_then(|v| v.as_str()), Some("tool_use_id"));
}

#[test]
fn facet_of_not_emitted_when_no_matching_tool_call() {
    let evs = vec![tool_log_ev("evt-log-orphan", "toolu_orphan")];
    let (_, edges) = compute("sess_t", &evs, &[], &[]);
    assert!(edges.iter().all(|e| e.edge_kind != "facet_of"));
}
