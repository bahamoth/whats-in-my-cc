use wimcc::insight::config::DetectorConfig;

#[test]
fn defaults_when_no_file() {
    let cfg = DetectorConfig::from_toml_str("");
    assert!(cfg.enabled("tool_failure"));
    assert_eq!(cfg.usize_param("tool_failure", "retry_window", 5), 5);
}

#[test]
fn pack_id_from_top_level_id_key() {
    let cfg = DetectorConfig::from_toml_str(
        "id = \"tuning@2026-07\"\n[detector.re_read]\nmin_reads = 3\n",
    );
    assert_eq!(cfg.pack_id(), Some("tuning@2026-07"));
    assert_eq!(cfg.usize_param("re_read", "min_reads", 2), 3);
    // id 없는 파일: 파라미터는 적용되지만 pack은 무명 — rule_pack은 null로 남는다.
    let anon = DetectorConfig::from_toml_str("[detector.re_read]\nmin_reads = 3\n");
    assert_eq!(anon.pack_id(), None);
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
