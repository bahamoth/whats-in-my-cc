use witmcc::insight::episode::rules::RULE_IDS;

#[test]
fn rule_ids_are_canonical() {
    let expected: &[&str] = &[
        "phase_intake_fresh_user_message@v1",
        "phase_exploration_read_only_window@v1",
        "phase_diagnosis_after_error@v1",
        "phase_action_first_mutation@v1",
        "phase_verification_run_window@v1",
        "phase_repair_after_failed_verification@v1",
        "phase_drift_long_exploration@v1",
    ];
    assert_eq!(RULE_IDS, expected);
}
