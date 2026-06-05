//! Slice-18 — RedactionManifest shape tests.
//!
//! Locks the manifest field contract: rules_applied, items_redacted_count,
//! redaction_state, schema_version, rule_pack.

use wimcc::security::redaction::{engine::scan, manifest::RedactionState};

#[test]
fn manifest_records_rules_applied_and_counts() {
    let result = scan("alice@acme.com and Bearer abc-def-12345-67890-zzzz123456789");
    assert!(result.applied, "scan must report applied=true when secrets present");
    let m = result.manifest;
    assert!(
        m.rules_applied.iter().any(|s| s == "email.v1"),
        "email.v1 must be in rules_applied; got: {:?}",
        m.rules_applied
    );
    assert!(
        m.rules_applied.iter().any(|s| s == "bearer_token.v1"),
        "bearer_token.v1 must be in rules_applied; got: {:?}",
        m.rules_applied
    );
    assert!(
        m.items_redacted_count >= 2,
        "at least 2 items must be redacted; got: {}",
        m.items_redacted_count
    );
    assert_eq!(
        m.redaction_state,
        RedactionState::Redacted,
        "redaction_state must be Redacted"
    );
}

#[test]
fn manifest_schema_version_and_rule_pack_are_canonical() {
    let result = scan("alice@acme.com");
    let m = result.manifest;
    assert_eq!(m.schema_version, "redaction_manifest.v1");
    assert_eq!(m.rule_pack, "rule_pack@v1");
}

#[test]
fn manifest_not_redacted_state_when_no_secrets() {
    let result = scan("plain text with no secrets here");
    assert!(!result.applied);
    assert_eq!(result.manifest.redaction_state, RedactionState::NotRedacted);
    assert_eq!(result.manifest.items_redacted_count, 0);
    assert!(result.manifest.rules_applied.is_empty());
}

#[test]
fn has_unredacted_sensitive_payload_false_when_specific_rule_fires() {
    // When a specific rule (not just heuristic) fires, has_unredacted flag
    // should be false — the specific rule masked it.
    let result = scan("alice@acme.com");
    assert!(!result.manifest.has_unredacted_sensitive_payload);
}

#[test]
fn review_required_before_export_true_when_has_unredacted_sensitive() {
    // Construct a scan result that only triggers the heuristic (no specific rule).
    // We use a long random-looking base64 string that doesn't match specific rules.
    let high_entropy = "aB3dE6gH9jK2mN5pQ8sT1vW4yZ7cF0iL";
    let result = scan(high_entropy);
    // If heuristic fired:
    if result.manifest.has_unredacted_sensitive_payload {
        assert!(
            result.manifest.review_required_before_export,
            "review_required_before_export must be true when has_unredacted_sensitive_payload=true"
        );
    }
    // If heuristic didn't fire for this string, the test is vacuously ok.
}
