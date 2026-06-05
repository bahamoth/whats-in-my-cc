mod common;

use common::edge_inference::synth_session_with_known_repair_pattern;
use wimcc::graph::build::compute;

#[test]
fn inferred_edges_carry_rule_id_and_confidence() {
    let evs = synth_session_with_known_repair_pattern();
    let (_, edges) = compute("sess_t", &evs, &[], &[]);
    let inferred: Vec<_> = edges
        .iter()
        .filter(|e| e.inference_rule_id.is_some())
        .collect();
    assert!(
        !inferred.is_empty(),
        "expected at least one inferred edge from the repair pattern"
    );
    for e in inferred {
        let conf = e.confidence.expect("inferred edge must have confidence");
        assert!(
            (0.0..=1.0).contains(&conf),
            "confidence {conf} must be in [0, 1]"
        );
        assert_eq!(e.origin, "inferred", "inferred edges must have origin=inferred");
    }
}
