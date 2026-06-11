//! Slice-10a — transcript-driven file lineage.
//!
//! Replaces the removed filesystem watcher + git poller. Extracts `DiffHunkRecord`s
//! from a transcript `tool_result`'s `toolUseResult.structuredPatch` array.
//! Only `Edit` produces hunks; `Write` always emits `structuredPatch: []` by
//! design — we return an empty Vec for it. `MultiEdit` is currently unobserved
//! in real transcripts and is therefore not extracted (would require a fresh
//! invariant fixture to lock).
//!
//! Invariant locked by `tests/transcript_structured_patch.rs` against real
//! fixtures in `tests/fixtures/transcripts/real/structured_patch_v01.jsonl`.

use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Truncate point for the `patch_preview` column. Kept bit-identical with the
/// slice-5 `file_git::PATCH_PREVIEW_MAX_BYTES` for behaviour continuity.
pub const PATCH_PREVIEW_MAX_BYTES: usize = 4 * 1024;

/// Mirror of the transcript-side `toolUseResult.structuredPatch[i]` shape.
/// Real-fixture-anchored — any drift here means a future Claude Code version
/// changed the schema and the invariant test will fail loudly.
#[derive(Debug, Clone, Deserialize)]
pub struct TranscriptHunk {
    #[serde(rename = "oldStart")]
    pub old_start: u32,
    #[serde(rename = "oldLines")]
    pub old_lines: u32,
    #[serde(rename = "newStart")]
    pub new_start: u32,
    #[serde(rename = "newLines")]
    pub new_lines: u32,
    pub lines: Vec<String>,
}

/// Mirror of the `toolUseResult` envelope.
#[derive(Debug, Clone, Deserialize)]
pub struct TranscriptToolUseResult {
    #[serde(rename = "filePath")]
    pub file_path: String,
    #[serde(rename = "structuredPatch", default)]
    pub structured_patch: Vec<TranscriptHunk>,
    #[serde(rename = "userModified", default)]
    pub user_modified: bool,
}

/// Normalised slice-10a diff hunk. One per transcript hunk. Persisted via
/// `repo_diff_hunk`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffHunkRecord {
    pub diff_hunk_id: String,
    pub session_id: String,
    pub file_path: String,
    /// "added" | "modified" | "deleted"
    pub change_type: String,
    /// `(start, end)` line range on the post-image side; `None` for empty hunks.
    pub line_range_after: Option<(u32, u32)>,
    pub introduced_by_event_id: String,
    pub introduced_by_tool_use_id: Option<String>,
    pub patch_preview: String,
    pub lines_added: u32,
    pub lines_removed: u32,
    pub user_modified: bool,
}

/// Extract `DiffHunkRecord`s from a transcript `tool_result` line's
/// `toolUseResult` value. Returns an empty Vec when the result has no
/// `structuredPatch` (Write / Bash / Read / etc.) — never errors on non-edit
/// tool types.
///
/// `event_id` is the `observed_event.event_id` of the persisted tool_result.
/// `tool_use_id` is the matching `tool_use.id` from the prior assistant turn.
pub fn extract_diff_hunks(
    event_id: &str,
    tool_use_id: Option<&str>,
    session_id: &str,
    tool_use_result: &Value,
) -> Vec<DiffHunkRecord> {
    let parsed: TranscriptToolUseResult = match serde_json::from_value(tool_use_result.clone()) {
        Ok(p) => p,
        Err(_) => return Vec::new(),
    };
    parsed
        .structured_patch
        .iter()
        .enumerate()
        .map(|(idx, h)| build_record(event_id, tool_use_id, session_id, &parsed, idx, h))
        .collect()
}

fn build_record(
    event_id: &str,
    tool_use_id: Option<&str>,
    session_id: &str,
    parent: &TranscriptToolUseResult,
    hunk_index: usize,
    h: &TranscriptHunk,
) -> DiffHunkRecord {
    let (lines_added, lines_removed) = count_added_removed(&h.lines);
    let change_type = classify_change(h.old_lines, h.new_lines);
    let line_range_after = if h.new_lines > 0 {
        Some((
            h.new_start,
            h.new_start.saturating_add(h.new_lines).saturating_sub(1),
        ))
    } else {
        None
    };
    DiffHunkRecord {
        diff_hunk_id: derive_id(event_id, hunk_index),
        session_id: session_id.to_string(),
        file_path: parent.file_path.clone(),
        change_type: change_type.into(),
        line_range_after,
        introduced_by_event_id: event_id.to_string(),
        introduced_by_tool_use_id: tool_use_id.map(|s| s.to_string()),
        patch_preview: truncate_patch_preview(&h.lines.join("\n")),
        lines_added,
        lines_removed,
        user_modified: parent.user_modified,
    }
}

fn count_added_removed(lines: &[String]) -> (u32, u32) {
    let mut added = 0u32;
    let mut removed = 0u32;
    for ln in lines {
        match ln.chars().next() {
            Some('+') => added = added.saturating_add(1),
            Some('-') => removed = removed.saturating_add(1),
            _ => {}
        }
    }
    (added, removed)
}

fn classify_change(old_lines: u32, new_lines: u32) -> &'static str {
    match (old_lines, new_lines) {
        (0, n) if n > 0 => "added",
        (o, 0) if o > 0 => "deleted",
        _ => "modified",
    }
}

fn derive_id(event_id: &str, hunk_index: usize) -> String {
    let mut h = Sha256::new();
    h.update(event_id.as_bytes());
    h.update(b"|hunk|");
    h.update(hunk_index.to_string().as_bytes());
    format!("dh_{}", hex::encode(h.finalize()))
}

/// Truncate `s` to at most `PATCH_PREVIEW_MAX_BYTES` on a UTF-8 boundary,
/// appending `\n…[truncated]` if anything was dropped. Bit-identical with the
/// retired `file_git::truncate_patch_preview`.
pub fn truncate_patch_preview(s: &str) -> String {
    if s.len() <= PATCH_PREVIEW_MAX_BYTES {
        return s.to_string();
    }
    let mut end = PATCH_PREVIEW_MAX_BYTES;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    let mut out = String::with_capacity(end + 16);
    out.push_str(&s[..end]);
    out.push_str("\n…[truncated]");
    out
}
