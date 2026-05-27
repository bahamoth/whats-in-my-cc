mod common;

use common::edge_inference::synth_tool_result_then_assistant_msg;
use witmcc::insight::edge_inference::rules::large_output_to_next_action_v1::LargeOutputToNextActionV1;
use witmcc::insight::edge_inference::EdgeInferenceRule;

#[test]
fn emits_edge_when_payload_exceeds_threshold() {
    let view = synth_tool_result_then_assistant_msg(100_000);
    let edges = LargeOutputToNextActionV1.infer(&view);
    assert_eq!(edges.len(), 1, "expected 1 inferred edge, got {}", edges.len());
    let e = &edges[0];
    assert_eq!(
        e.inference_rule_id.as_deref(),
        Some("large_output_to_next_action@v1"),
        "wrong rule_id"
    );
    let size = e
        .attributes
        .get("tool_result_size_bytes")
        .and_then(|v| v.as_i64())
        .expect("tool_result_size_bytes must be present");
    assert_eq!(size, 100_000, "wrong size attribute");
}

#[test]
fn does_not_emit_when_payload_below_threshold() {
    let view = synth_tool_result_then_assistant_msg(10);
    let edges = LargeOutputToNextActionV1.infer(&view);
    assert!(
        edges.is_empty(),
        "should not fire when payload below 50KB threshold"
    );
}
