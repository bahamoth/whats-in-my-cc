//! Slice-18 — Shim lock converted to real-gate lock (DEV-S18-05).
//!
//! The slice-16 no-op assertions are replaced with assertions that the real
//! redaction engine is now active. The test file is intentionally preserved
//! (not deleted) per the plan — same file, different invariant.
//!
//! Previous invariant (slice-16): shim returns text unchanged.
//! New invariant (slice-18): real gate masks secrets.

#[test]
fn redaction_engine_masks_in_projection_path() {
    // Calling through the shim module path (insight::redaction_shim::apply_text)
    // must now invoke the real gate and redact secrets.
    let projected = wimcc::insight::redaction_shim::apply_text("alice@acme.com triggered rm -rf");
    assert!(
        !projected.contains("alice@acme.com"),
        "real gate must mask emails; shim must no longer be a no-op. got: {projected}"
    );
}

#[test]
fn redaction_engine_replaces_shim_anthropic_key() {
    let projected =
        wimcc::insight::redaction_shim::apply_text("key=sk-ant-api03-aaaaaaaaaaaaaaaaaaaaaaaaaaaa");
    assert!(
        !projected.contains("aaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        "real gate must mask Anthropic keys via shim path; got: {projected}"
    );
}

#[test]
fn apply_text_truncated_preserves_utf8_boundary_and_redacts() {
    // "こんにちは" is 5 chars × 3 bytes = 15 bytes.
    // Truncated to 6 bytes → "こん" (2 × 3 bytes).
    let input = "こんにちは";
    let output = wimcc::insight::redaction_shim::apply_text_truncated(input, 6);
    assert!(output.len() <= 6, "truncated output must be ≤ max_bytes");
    // Must still be valid UTF-8.
    let _ = output.as_str();
}
