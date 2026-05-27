mod common;

use common::edge_inference::{
    synth_user_then_assistant_text_then_tool, synth_user_then_tool_no_assistant_text,
};
use witmcc::insight::edge_inference::rules::triggered_by_user_message_v1::TriggeredByUserMessageV1;
use witmcc::insight::edge_inference::EdgeInferenceRule;

#[test]
fn emits_edge_when_assistant_skipped_text() {
    let view = synth_user_then_tool_no_assistant_text();
    let edges = TriggeredByUserMessageV1.infer(&view);
    assert_eq!(edges.len(), 1, "expected 1 inferred edge, got {}", edges.len());
    let e = &edges[0];
    assert_eq!(
        e.inference_rule_id.as_deref(),
        Some("triggered_by_user_message@v1"),
        "wrong rule_id"
    );
    let c = e.confidence.expect("confidence must be set");
    assert!(
        (c - 0.85f32).abs() < 0.001,
        "confidence should be 0.85, got {c}"
    );
}

#[test]
fn does_not_emit_when_assistant_text_preceded() {
    let view = synth_user_then_assistant_text_then_tool();
    let edges = TriggeredByUserMessageV1.infer(&view);
    assert!(
        edges.is_empty(),
        "should not fire when assistant text precedes tool_call"
    );
}
