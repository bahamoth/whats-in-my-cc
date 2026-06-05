//! Synthetic helpers for edge inference tests (slice-13).

use chrono::{TimeZone, Utc};
use serde_json::json;
use wimcc::graph::build::compute;
use wimcc::insight::edge_inference::SessionGraphView;
use wimcc::model::graph::{GraphEdge, GraphNode};
use wimcc::model::observed::{Actor, EventKind, ObservedEvent};

/// Build a base ObservedEvent with index-derived fields.
pub fn ev(
    i: usize,
    actor: Actor,
    kind: EventKind,
    tool: Option<&str>,
    tool_use_id: Option<&str>,
) -> ObservedEvent {
    ObservedEvent {
        event_id: format!("ev_{i:03}"),
        raw_event_id: format!("raw_{i:03}"),
        schema_version: "observed_event.v1".into(),
        session_id: "sess_t".into(),
        event_uuid: Some(format!("uuid_{i}")),
        parent_uuid: if i > 0 {
            Some(format!("uuid_{}", i - 1))
        } else {
            None
        },
        observed_at: Utc.timestamp_opt(1_700_000_000 + i as i64 * 10, 0).unwrap(),
        actor,
        kind,
        tool_name: tool.map(String::from),
        tool_use_id: tool_use_id.map(String::from),
        parser_version: "test".into(),
        ..Default::default()
    }
}

/// A session with: tool_call (error result) followed quickly by another
/// tool_call whose input text shares tokens with the error message.
pub fn synth_view_with_error_then_repair(
    error_text: &str,
    repair_input: &str,
) -> SessionGraphView<'static> {
    synth_view_internal_repair(error_text, repair_input, 10)
}

/// A session where the repair tool_call is delayed `delay_secs` past the window.
pub fn synth_view_with_error_then_delayed_repair(delay_secs: i64) -> SessionGraphView<'static> {
    synth_view_internal_delayed(delay_secs)
}

fn make_static_events(
    error_text: &str,
    repair_input: &str,
    delay_secs: i64,
) -> (Vec<GraphNode>, Vec<GraphEdge>, Vec<ObservedEvent>) {
    let t0 = 1_700_000_000i64;
    let call_ev = ObservedEvent {
        event_id: "ev_000".into(),
        raw_event_id: "raw_000".into(),
        schema_version: "observed_event.v1".into(),
        session_id: "sess_t".into(),
        event_uuid: Some("uuid_0".into()),
        parent_uuid: None,
        observed_at: Utc.timestamp_opt(t0, 0).unwrap(),
        actor: Actor::Assistant,
        kind: EventKind::ToolCall,
        tool_name: Some("Bash".into()),
        tool_use_id: Some("tid_0".into()),
        payload: json!({ "tool_use_id": "tid_0", "name": "Bash", "input": { "command": "run something" } }),
        parser_version: "test".into(),
        ..Default::default()
    };
    let result_ev = ObservedEvent {
        event_id: "ev_001".into(),
        raw_event_id: "raw_001".into(),
        schema_version: "observed_event.v1".into(),
        session_id: "sess_t".into(),
        event_uuid: Some("uuid_1".into()),
        parent_uuid: Some("uuid_0".into()),
        observed_at: Utc.timestamp_opt(t0 + 5, 0).unwrap(),
        actor: Actor::Tool,
        kind: EventKind::ToolResult,
        tool_use_id: Some("tid_0".into()),
        payload: json!({
            "tool_use_id": "tid_0",
            "is_error": true,
            "content": [{ "type": "text", "text": error_text }]
        }),
        parser_version: "test".into(),
        ..Default::default()
    };
    let repair_ev = ObservedEvent {
        event_id: "ev_002".into(),
        raw_event_id: "raw_002".into(),
        schema_version: "observed_event.v1".into(),
        session_id: "sess_t".into(),
        event_uuid: Some("uuid_2".into()),
        parent_uuid: Some("uuid_1".into()),
        observed_at: Utc.timestamp_opt(t0 + 5 + delay_secs, 0).unwrap(),
        actor: Actor::Assistant,
        kind: EventKind::ToolCall,
        tool_name: Some("Edit".into()),
        tool_use_id: Some("tid_2".into()),
        payload: json!({
            "tool_use_id": "tid_2",
            "name": "Edit",
            "input": { "new_content": repair_input }
        }),
        parser_version: "test".into(),
        ..Default::default()
    };
    let evs = vec![call_ev, result_ev, repair_ev];
    let (nodes, edges) = compute("sess_t", &evs, &[], &[]);
    (nodes, edges, evs)
}

fn synth_view_internal_repair(
    error_text: &str,
    repair_input: &str,
    delay_secs: i64,
) -> SessionGraphView<'static> {
    let (nodes, edges, events) = make_static_events(error_text, repair_input, delay_secs);
    // We need 'static lifetime for the helper — leak the vecs for test simplicity.
    let nodes_static: &'static [GraphNode] = Box::leak(nodes.into_boxed_slice());
    let edges_static: &'static [GraphEdge] = Box::leak(edges.into_boxed_slice());
    let events_static: &'static [ObservedEvent] = Box::leak(events.into_boxed_slice());
    SessionGraphView {
        session_id: "sess_t",
        events: events_static,
        nodes: nodes_static,
        deterministic_edges: edges_static,
    }
}

fn synth_view_internal_delayed(delay_secs: i64) -> SessionGraphView<'static> {
    // "boom" vs "list files in /tmp" — no overlap expected
    let error_text = "boom";
    let repair_input = "list files in /tmp";
    synth_view_internal_repair(error_text, repair_input, delay_secs)
}

/// A session: user_message → tool_call (no assistant_message in between).
/// `triggered_by_user_message@v1` should fire.
pub fn synth_user_then_tool_no_assistant_text() -> SessionGraphView<'static> {
    let t0 = 1_700_000_000i64;
    let user_ev = ObservedEvent {
        event_id: "ev_000".into(),
        raw_event_id: "raw_000".into(),
        schema_version: "observed_event.v1".into(),
        session_id: "sess_t".into(),
        event_uuid: Some("uuid_0".into()),
        observed_at: Utc.timestamp_opt(t0, 0).unwrap(),
        actor: Actor::User,
        kind: EventKind::UserMessage,
        parser_version: "test".into(),
        payload: json!({"role": "user", "content": [{"type":"text","text":"do it"}]}),
        ..Default::default()
    };
    let tool_ev = ObservedEvent {
        event_id: "ev_001".into(),
        raw_event_id: "raw_001".into(),
        schema_version: "observed_event.v1".into(),
        session_id: "sess_t".into(),
        event_uuid: Some("uuid_1".into()),
        // No parent_uuid from user_ev — same turn but no text between
        observed_at: Utc.timestamp_opt(t0 + 5, 0).unwrap(),
        actor: Actor::Assistant,
        kind: EventKind::ToolCall,
        tool_name: Some("Bash".into()),
        tool_use_id: Some("tid_1".into()),
        payload: json!({"tool_use_id": "tid_1", "name": "Bash", "input": {"command": "ls"}}),
        parser_version: "test".into(),
        ..Default::default()
    };
    let evs = vec![user_ev, tool_ev];
    let (nodes, edges) = compute("sess_t", &evs, &[], &[]);
    let nodes_static: &'static [GraphNode] = Box::leak(nodes.into_boxed_slice());
    let edges_static: &'static [GraphEdge] = Box::leak(edges.into_boxed_slice());
    let events_static: &'static [ObservedEvent] = Box::leak(evs.into_boxed_slice());
    SessionGraphView {
        session_id: "sess_t",
        events: events_static,
        nodes: nodes_static,
        deterministic_edges: edges_static,
    }
}

/// A session: user_message → assistant_message (text) → tool_call.
/// `triggered_by_user_message@v1` should NOT fire.
pub fn synth_user_then_assistant_text_then_tool() -> SessionGraphView<'static> {
    let t0 = 1_700_000_000i64;
    let user_ev = ObservedEvent {
        event_id: "ev_000".into(),
        raw_event_id: "raw_000".into(),
        schema_version: "observed_event.v1".into(),
        session_id: "sess_t".into(),
        event_uuid: Some("uuid_0".into()),
        observed_at: Utc.timestamp_opt(t0, 0).unwrap(),
        actor: Actor::User,
        kind: EventKind::UserMessage,
        payload: json!({"role": "user", "content": [{"type":"text","text":"do it"}]}),
        parser_version: "test".into(),
        ..Default::default()
    };
    let asst_ev = ObservedEvent {
        event_id: "ev_001".into(),
        raw_event_id: "raw_001".into(),
        schema_version: "observed_event.v1".into(),
        session_id: "sess_t".into(),
        event_uuid: Some("uuid_1".into()),
        parent_uuid: Some("uuid_0".into()),
        observed_at: Utc.timestamp_opt(t0 + 3, 0).unwrap(),
        actor: Actor::Assistant,
        kind: EventKind::AssistantMessage,
        payload: json!({"role": "assistant", "content": [{"type":"text","text":"Sure"}]}),
        parser_version: "test".into(),
        ..Default::default()
    };
    let tool_ev = ObservedEvent {
        event_id: "ev_002".into(),
        raw_event_id: "raw_002".into(),
        schema_version: "observed_event.v1".into(),
        session_id: "sess_t".into(),
        event_uuid: Some("uuid_2".into()),
        parent_uuid: Some("uuid_1".into()),
        observed_at: Utc.timestamp_opt(t0 + 6, 0).unwrap(),
        actor: Actor::Assistant,
        kind: EventKind::ToolCall,
        tool_name: Some("Bash".into()),
        tool_use_id: Some("tid_2".into()),
        payload: json!({"tool_use_id": "tid_2", "name": "Bash", "input": {"command": "ls"}}),
        parser_version: "test".into(),
        ..Default::default()
    };
    let evs = vec![user_ev, asst_ev, tool_ev];
    let (nodes, edges) = compute("sess_t", &evs, &[], &[]);
    let nodes_static: &'static [GraphNode] = Box::leak(nodes.into_boxed_slice());
    let edges_static: &'static [GraphEdge] = Box::leak(edges.into_boxed_slice());
    let events_static: &'static [ObservedEvent] = Box::leak(evs.into_boxed_slice());
    SessionGraphView {
        session_id: "sess_t",
        events: events_static,
        nodes: nodes_static,
        deterministic_edges: edges_static,
    }
}

/// A session: tool_result with `payload_bytes` of data, followed by assistant_message.
pub fn synth_tool_result_then_assistant_msg(payload_bytes: usize) -> SessionGraphView<'static> {
    let t0 = 1_700_000_000i64;
    // Pad the payload with 'x' bytes of text
    let content_text = "x".repeat(payload_bytes);
    let call_ev = ObservedEvent {
        event_id: "ev_000".into(),
        raw_event_id: "raw_000".into(),
        schema_version: "observed_event.v1".into(),
        session_id: "sess_t".into(),
        event_uuid: Some("uuid_0".into()),
        observed_at: Utc.timestamp_opt(t0, 0).unwrap(),
        actor: Actor::Assistant,
        kind: EventKind::ToolCall,
        tool_name: Some("Bash".into()),
        tool_use_id: Some("tid_0".into()),
        payload: json!({"tool_use_id": "tid_0", "name": "Bash", "input": {"command": "cat big_file"}}),
        parser_version: "test".into(),
        ..Default::default()
    };
    let result_ev = ObservedEvent {
        event_id: "ev_001".into(),
        raw_event_id: "raw_001".into(),
        schema_version: "observed_event.v1".into(),
        session_id: "sess_t".into(),
        event_uuid: Some("uuid_1".into()),
        parent_uuid: Some("uuid_0".into()),
        observed_at: Utc.timestamp_opt(t0 + 5, 0).unwrap(),
        actor: Actor::Tool,
        kind: EventKind::ToolResult,
        tool_use_id: Some("tid_0".into()),
        payload: json!({
            "tool_use_id": "tid_0",
            "content": [{ "type": "text", "text": content_text }]
        }),
        parser_version: "test".into(),
        ..Default::default()
    };
    let asst_ev = ObservedEvent {
        event_id: "ev_002".into(),
        raw_event_id: "raw_002".into(),
        schema_version: "observed_event.v1".into(),
        session_id: "sess_t".into(),
        event_uuid: Some("uuid_2".into()),
        parent_uuid: Some("uuid_1".into()),
        observed_at: Utc.timestamp_opt(t0 + 10, 0).unwrap(),
        actor: Actor::Assistant,
        kind: EventKind::AssistantMessage,
        payload: json!({"role":"assistant","content":[{"type":"text","text":"Here is the content"}]}),
        parser_version: "test".into(),
        ..Default::default()
    };
    let evs = vec![call_ev, result_ev, asst_ev];
    let (nodes, edges) = compute("sess_t", &evs, &[], &[]);
    let nodes_static: &'static [GraphNode] = Box::leak(nodes.into_boxed_slice());
    let edges_static: &'static [GraphEdge] = Box::leak(edges.into_boxed_slice());
    let events_static: &'static [ObservedEvent] = Box::leak(evs.into_boxed_slice());
    SessionGraphView {
        session_id: "sess_t",
        events: events_static,
        nodes: nodes_static,
        deterministic_edges: edges_static,
    }
}

/// Synthetic session with a repair pattern for compute() integration test.
pub fn synth_session_with_known_repair_pattern() -> Vec<ObservedEvent> {
    let t0 = 1_700_000_000i64;
    vec![
        ObservedEvent {
            event_id: "ev_000".into(),
            raw_event_id: "raw_000".into(),
            schema_version: "observed_event.v1".into(),
            session_id: "sess_t".into(),
            event_uuid: Some("uuid_0".into()),
            observed_at: Utc.timestamp_opt(t0, 0).unwrap(),
            actor: Actor::Assistant,
            kind: EventKind::ToolCall,
            tool_name: Some("Bash".into()),
            tool_use_id: Some("tid_0".into()),
            payload: json!({"tool_use_id": "tid_0", "name": "Bash", "input": {"command": "cargo build"}}),
            parser_version: "test".into(),
            ..Default::default()
        },
        ObservedEvent {
            event_id: "ev_001".into(),
            raw_event_id: "raw_001".into(),
            schema_version: "observed_event.v1".into(),
            session_id: "sess_t".into(),
            event_uuid: Some("uuid_1".into()),
            parent_uuid: Some("uuid_0".into()),
            observed_at: Utc.timestamp_opt(t0 + 5, 0).unwrap(),
            actor: Actor::Tool,
            kind: EventKind::ToolResult,
            tool_use_id: Some("tid_0".into()),
            payload: json!({
                "tool_use_id": "tid_0",
                "is_error": true,
                "content": [{ "type": "text", "text": "error[E0308]: mismatched types in cargo build" }]
            }),
            parser_version: "test".into(),
            ..Default::default()
        },
        ObservedEvent {
            event_id: "ev_002".into(),
            raw_event_id: "raw_002".into(),
            schema_version: "observed_event.v1".into(),
            session_id: "sess_t".into(),
            event_uuid: Some("uuid_2".into()),
            parent_uuid: Some("uuid_1".into()),
            observed_at: Utc.timestamp_opt(t0 + 15, 0).unwrap(),
            actor: Actor::Assistant,
            kind: EventKind::ToolCall,
            tool_name: Some("Edit".into()),
            tool_use_id: Some("tid_2".into()),
            payload: json!({
                "tool_use_id": "tid_2",
                "name": "Edit",
                "input": { "new_content": "fix the mismatched types cargo" }
            }),
            parser_version: "test".into(),
            ..Default::default()
        },
    ]
}
