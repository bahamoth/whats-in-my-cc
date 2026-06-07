use wimcc::insight::config::DetectorConfig;

#[test]
fn defaults_when_no_file() {
    let cfg = DetectorConfig::from_toml_str("");
    assert!(cfg.enabled("tool_failure"));
    assert_eq!(cfg.usize_param("tool_failure", "retry_window", 5), 5);
}

#[test]
fn override_and_fallback() {
    let cfg = DetectorConfig::from_toml_str(
        "[detector.tool_failure]\nenabled = false\nretry_window = 9\n",
    );
    assert!(!cfg.enabled("tool_failure"));
    assert_eq!(cfg.usize_param("tool_failure", "retry_window", 5), 9);
    assert!(cfg.enabled("risky_action"));
    assert_eq!(cfg.usize_param("risky_action", "window", 7), 7);
}
