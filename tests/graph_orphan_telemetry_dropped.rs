//! Slice 2 (telemetry fold) — orphan telemetry dropped from the graph.
//!
//! After the Slice 1 fold (which folds *foldable* telemetry — llm_request span
//! + api_request/tool_result/tool_decision logs — into their owner node's
//! payload.facets), `compute()` must additionally drop the REMAINING orphan
//! telemetry nodes that have no owner and no backbone role:
//!   metric_sample, hook_event, otel_span (non-llm_request), log_record (non-fold).
//!
//! The graph is left with only the conversation/action backbone. Telemetry data
//! itself stays in observed_event/raw_event (SSOT); compute() does not touch
//! those tables.
//!
//! This single stream exercises BOTH passes in one graph so it locks the
//! fold-before-drop ordering: the foldable llm_request span and tool_result log
//! must survive as FACETS (proving fold ran first), while the orphan span /
//! orphan log / metric / hook nodes must be GONE.

mod common;

use serde_json::{json, Value};
use wimcc::graph::build::compute;
use wimcc::model::observed::{EventKind, ObservedEvent};

fn user_ev(event_id: &str) -> ObservedEvent {
    let mut e = common::base_event(EventKind::UserMessage, event_id);
    e.payload = json!({"role":"user","content":[{"type":"text","text":"hi"}]});
    e
}

fn assistant_ev(event_id: &str, rid: &str) -> ObservedEvent {
    let mut e = common::base_event(EventKind::AssistantMessage, event_id);
    e.request_id = Some(rid.into());
    e.payload = json!({"role":"assistant","content":[]});
    e
}

fn llm_span_ev(event_id: &str, rid: &str) -> ObservedEvent {
    // FOLDABLE — claude_code.llm_request span with request_id in raw_span.attributes[].
    let mut e = common::base_event(EventKind::OtelSpan, event_id);
    e.trace_id = Some("trace-fold".into());
    e.span_id = Some("span-fold".into());
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

fn tool_call_ev(event_id: &str, tuid: &str) -> ObservedEvent {
    let mut e = common::base_event(EventKind::ToolCall, event_id);
    e.tool_use_id = Some(tuid.into());
    e.tool_name = Some("Bash".into());
    e.payload = json!({"tool_name":"Bash","input":{"command":"ls"}});
    e
}

fn tool_result_log_ev(event_id: &str, tuid: &str) -> ObservedEvent {
    // FOLDABLE — tool_result log keyed by tool_use_id.
    let mut e = common::base_event(EventKind::LogRecord, event_id);
    e.payload = json!({
        "event_name":"tool_result",
        "attributes":{"tool_use_id":tuid,"duration_ms":"57","success":"true"}
    });
    e
}

fn orphan_span_ev(event_id: &str) -> ObservedEvent {
    // ORPHAN — non-llm_request span, no owner. Must be dropped.
    let mut e = common::base_event(EventKind::OtelSpan, event_id);
    e.trace_id = Some("trace-orphan".into());
    e.span_id = Some("span-orphan".into());
    e.payload = json!({
        "raw_span":{
            "name":"claude_code.tool.execution",
            "attributes":[
                {"key":"tool.name","value":{"stringValue":"Bash"}}
            ]
        }
    });
    e
}

fn orphan_log_ev(event_id: &str) -> ObservedEvent {
    // ORPHAN — generic hook log with no tool_use_id / request_id. Must be dropped.
    let mut e = common::base_event(EventKind::LogRecord, event_id);
    e.payload = json!({
        "event_name":"hook_execution_complete",
        "attributes":{"hook":"PostToolUse","status":"ok"}
    });
    e
}

fn metric_ev(event_id: &str) -> ObservedEvent {
    // ORPHAN — metric data point. Must be dropped (metric facet is a future slice).
    let mut e = common::base_event(EventKind::MetricSample, event_id);
    e.payload = json!({
        "instrument_name":"claude_code.cost.usage",
        "value_float":0.1,
        "time_unix_nano":"1700000001000000000"
    });
    e
}

fn hook_ev(event_id: &str) -> ObservedEvent {
    // ORPHAN — hook_event node. Must be dropped.
    let mut e = common::base_event(EventKind::HookEvent, event_id);
    e.subkind = Some("PostToolUse".into());
    e.payload = json!({"hook":{"hook_event_name":"PostToolUse"}});
    e
}

#[test]
fn orphan_telemetry_dropped_after_fold_preserved() {
    let evs = vec![
        user_ev("evt-user"),
        assistant_ev("evt-asst", "req_A"),
        llm_span_ev("evt-span-fold", "req_A"),
        tool_call_ev("evt-call", "tu_A"),
        tool_result_log_ev("evt-res", "tu_A"),
        orphan_span_ev("evt-span-orphan"),
        orphan_log_ev("evt-log-orphan"),
        metric_ev("evt-metric"),
        hook_ev("evt-hook"),
    ];
    let (nodes, edges) = compute("sess_orphan", &evs, &[], &[]);

    // 1. Fold preserved — proves the Slice-1 fold ran BEFORE the Slice-2 drop.
    //    The foldable llm_request span ended up as a facet on the assistant,
    //    and the tool_result log as a facet on the tool_call — not dropped first.
    let asst = nodes
        .iter()
        .find(|n| n.node_kind == "assistant_message")
        .expect("assistant_message node present");
    let asst_fkinds: Vec<&str> = asst
        .payload
        .get("facets")
        .and_then(|f| f.as_array())
        .expect("assistant_message has payload.facets")
        .iter()
        .filter_map(|f| f.get("facet_kind").and_then(Value::as_str))
        .collect();
    assert!(
        asst_fkinds.contains(&"llm_request_span"),
        "llm_request span must be folded onto assistant (fold-before-drop); got {asst_fkinds:?}"
    );

    let call = nodes
        .iter()
        .find(|n| n.node_kind == "tool_call")
        .expect("tool_call node present");
    let call_fkinds: Vec<&str> = call
        .payload
        .get("facets")
        .and_then(|f| f.as_array())
        .expect("tool_call has payload.facets")
        .iter()
        .filter_map(|f| f.get("facet_kind").and_then(Value::as_str))
        .collect();
    assert!(
        call_fkinds.contains(&"tool_result_log"),
        "tool_result log must be folded onto tool_call (fold-before-drop); got {call_fkinds:?}"
    );

    // 2. Orphans gone — no telemetry node kinds survive in the graph.
    for kind in ["metric_sample", "hook_event", "otel_span", "log_record"] {
        assert!(
            !nodes.iter().any(|n| n.node_kind == kind),
            "no {kind} node may remain after Slice-2 drop; nodes={:?}",
            nodes.iter().map(|n| n.node_kind.as_str()).collect::<Vec<_>>()
        );
    }

    // 3. Backbone intact.
    for kind in ["user_message", "assistant_message", "tool_call"] {
        assert!(
            nodes.iter().any(|n| n.node_kind == kind),
            "backbone {kind} node must remain"
        );
    }

    // 4. No dangling edges — no edge references a dropped node id.
    let live: std::collections::HashSet<&str> =
        nodes.iter().map(|n| n.node_id.as_str()).collect();
    for e in &edges {
        assert!(
            live.contains(e.from_node_id.as_str()),
            "edge {} references dropped from-node {}",
            e.edge_id,
            e.from_node_id
        );
        assert!(
            live.contains(e.to_node_id.as_str()),
            "edge {} references dropped to-node {}",
            e.edge_id,
            e.to_node_id
        );
    }
}
