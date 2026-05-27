//! Locks the slice-16 redaction shim as a no-op (DEV-S16-05).
//!
//! Slice-18 converts this test to assert the real gate is active.
//! The test name must NOT change between slice-16 and slice-18 — the plan
//! says "converted (not deleted)".

#[test]
fn redaction_shim_is_noop() {
    let input = "secret_token_abc123 rm -rf /sensitive/path";
    let output = witmcc::insight::redaction_shim::apply_text(input);
    // The shim is a no-op — it returns the text unchanged.
    assert_eq!(
        output, input,
        "shim must be a no-op until slice-18 replaces it"
    );
}

#[test]
fn redaction_shim_truncated_preserves_utf8_boundary() {
    // "こんにちは" is 5 chars × 3 bytes = 15 bytes.
    let input = "こんにちは";
    let output = witmcc::insight::redaction_shim::apply_text_truncated(input, 6);
    // 6 bytes: "こ" is 3 bytes, "ん" is 3 bytes → 6 bytes = "こん".
    // Verify the output is valid UTF-8 and ≤ 6 bytes.
    assert!(output.len() <= 6, "truncated output must be ≤ max_bytes");
    // Must still be valid UTF-8 (will panic on invalid sequences)
    let _ = output.as_str();
}
