mod common;

use common::edge_inference::{
    synth_view_with_error_then_repair, synth_view_with_error_then_delayed_repair,
};
use wimcc::insight::edge_inference::rules::caused_repair_v1::CausedRepairV1;
use wimcc::insight::edge_inference::EdgeInferenceRule;

#[test]
fn emits_edge_when_error_text_overlaps_next_call_input() {
    let view = synth_view_with_error_then_repair(
        "AttributeError: 'User' object has no attribute 'is_admin'",
        "fix the is_admin attribute on User",
    );
    let edges = CausedRepairV1.infer(&view);
    assert_eq!(edges.len(), 1, "expected 1 inferred edge, got {}", edges.len());
    let e = &edges[0];
    assert_eq!(
        e.inference_rule_id.as_deref(),
        Some("caused_repair@v1"),
        "wrong rule_id"
    );
    let c = e.confidence.expect("confidence must be set");
    assert!(
        (0.0..=1.0).contains(&c),
        "confidence {c} out of range [0, 1]"
    );
}

#[test]
fn does_not_emit_when_no_token_overlap() {
    let view = synth_view_with_error_then_repair("boom", "list files in /tmp");
    let edges = CausedRepairV1.infer(&view);
    assert!(edges.is_empty(), "expected no edges when no token overlap");
}

#[test]
fn does_not_emit_when_repair_too_late() {
    // 120 s gap > the N=60 window
    let view = synth_view_with_error_then_delayed_repair(120);
    let edges = CausedRepairV1.infer(&view);
    assert!(edges.is_empty(), "delayed repair must not produce an edge");
}
