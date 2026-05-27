use witmcc::insight::edge_inference::RULE_IDS;

#[test]
fn rule_ids_are_canonical() {
    let expected: &[&str] = &[
        "caused_repair@v1",
        "triggered_by_user_message@v1",
        "large_output_to_next_action@v1",
    ];
    assert_eq!(RULE_IDS, expected);
}
