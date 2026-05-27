//! Slice-18: redaction shim replaced by real gate.
//!
//! The slice-16 no-op shim (DEV-S16-05) is now replaced by a re-export of
//! the real redaction engine. The tracing warn is removed.
//!
//! The shim module (`insight::redaction_shim`) is preserved for backward
//! compat with call sites in the insight pipeline that reference it. The
//! real engine lives in `security::redaction::engine`.
//!
//! `tests/redaction_shim_lock.rs` is updated (DEV-S18-05) to assert the
//! real gate is active rather than the previous no-op assertion.

pub use crate::security::redaction::engine::apply_text;

/// Truncate text to `max_bytes` bytes (UTF-8 safe) then apply redaction.
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
