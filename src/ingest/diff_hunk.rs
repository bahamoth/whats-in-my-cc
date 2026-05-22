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
    _event_id: &str,
    _tool_use_id: Option<&str>,
    _session_id: &str,
    _tool_use_result: &Value,
) -> Vec<DiffHunkRecord> {
    // Phase 1 stub — green body lands in Phase 2 (commit 2).
    unimplemented!("extract_diff_hunks body is added in slice-10a Phase 2 (commit 2)")
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
