//! Slice-14 — locks that the extractor registry contains exactly the two L1
//! categories in the defined order. Adding / removing categories without
//! updating this invariant fails the test.

#[test]
fn registry_contains_l1_categories_only_for_slice14() {
    let cats: Vec<&str> = witmcc::insight::registry::all_extractors()
        .iter()
        .map(|e| e.category())
        .collect();
    let expected = vec!["missing_verification", "tool_failure"];
    assert_eq!(cats, expected, "registry order/content must match expected");
}

#[test]
fn registry_floor_values() {
    let exts = witmcc::insight::registry::all_extractors();
    let mv = exts.iter().find(|e| e.category() == "missing_verification").unwrap();
    let tf = exts.iter().find(|e| e.category() == "tool_failure").unwrap();
    // Architecture spec §3: missing_verification = 0.9, tool_failure = 1.0
    assert!((mv.floor() - 0.9).abs() < f32::EPSILON, "mv floor must be 0.9, got {}", mv.floor());
    assert!((tf.floor() - 1.0).abs() < f32::EPSILON, "tf floor must be 1.0, got {}", tf.floor());
}
