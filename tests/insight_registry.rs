//! Locks that the extractor registry contains exactly the 4 MVP categories
//! in the defined order. Adding/removing categories without updating this
//! invariant fails the test (per architecture spec §4).
//! (missing_verification was removed with the episode/phase system.)

#[test]
fn registry_contains_all_mvp_categories_in_locked_order() {
    let cats: Vec<&str> = witmcc::insight::registry::all_extractors()
        .iter()
        .map(|e| e.category())
        .collect();
    let expected = vec![
        "tool_failure",
        "risky_action",
        "context_bloat",
        "final_state_mismatch",
    ];
    assert_eq!(cats, expected, "registry order/content must match expected");
}

#[test]
fn registry_floor_values() {
    let exts = witmcc::insight::registry::all_extractors();
    let tf = exts.iter().find(|e| e.category() == "tool_failure").unwrap();
    let ra = exts.iter().find(|e| e.category() == "risky_action").unwrap();
    let cb = exts.iter().find(|e| e.category() == "context_bloat").unwrap();
    let fsm = exts.iter().find(|e| e.category() == "final_state_mismatch").unwrap();
    // Architecture spec §3 confidence policy
    assert!((tf.floor() - 1.0).abs() < f32::EPSILON, "tf floor must be 1.0, got {}", tf.floor());
    assert!((ra.floor() - 0.7).abs() < f32::EPSILON, "ra floor must be 0.7, got {}", ra.floor());
    assert!((cb.floor() - 0.5).abs() < f32::EPSILON, "cb floor must be 0.5, got {}", cb.floor());
    assert!((fsm.floor() - 0.6).abs() < f32::EPSILON, "fsm floor must be 0.6, got {}", fsm.floor());
}
