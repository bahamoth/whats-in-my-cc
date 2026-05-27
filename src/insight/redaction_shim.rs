//! Redaction shim — temporary no-op placeholder (slice-16, DEV-S16-05).
//!
//! Slice-18 replaces this with the real redaction gate that applies
//! `rule_pack@v1` patterns. Until then, this shim passes text through
//! unchanged while emitting a tracing warn per call so the lapse is
//! observable in logs.
//!
//! The `tests/redaction_shim_lock.rs` test asserts this is a no-op;
//! slice-18 converts that test to assert the real gate is active.

/// Apply redaction to a text field before it is placed in an evidence
/// projection sent to the LLM judge.
///
/// # Current behaviour (shim)
/// Returns the input unchanged. Emits `tracing::warn!` so every call
/// is visible in development logs, reminding maintainers to complete
/// slice-18 before production use.
pub fn apply_text(text: &str) -> String {
    tracing::warn!(
        text_len = text.len(),
        "redaction_shim_invoked: text passed through without redaction (slice-18 pending)"
    );
    text.to_string()
}

/// Truncate text to `max_bytes` bytes (UTF-8 safe) then apply the shim.
pub fn apply_text_truncated(text: &str, max_bytes: usize) -> String {
    let truncated = if text.len() <= max_bytes {
        text
    } else {
        // Find the largest byte boundary ≤ max_bytes that is a valid char boundary.
        let mut end = max_bytes;
        while end > 0 && !text.is_char_boundary(end) {
            end -= 1;
        }
        &text[..end]
    };
    apply_text(truncated)
}
