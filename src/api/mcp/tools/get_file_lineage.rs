//! Slice-17 — MCP tool: whats_in_my_cc.get_file_lineage

use serde_json::{json, Value};
use sqlx::SqlitePool;

use crate::api::mcp::tools::{tool_error, tool_success};
use crate::db::repo_diff_hunk;

pub async fn call(args: &Value, pool: &SqlitePool) -> Value {
    let session_id = match args["session_id"].as_str() {
        Some(s) => s,
        None => return tool_error("missing required argument: session_id"),
    };
    let file_path = match args["file_path"].as_str() {
        Some(s) => s,
        None => return tool_error("missing required argument: file_path"),
    };

    let hunks = match repo_diff_hunk::list_session(pool, session_id).await {
        Ok(h) => h,
        Err(e) => return tool_error(format!("db error: {e}")),
    };

    // Filter hunks for the requested file path.
    let filtered: Vec<Value> = hunks
        .into_iter()
        .filter(|h| h.file_path == file_path)
        .map(|h| json!({
            "diff_hunk_id": h.diff_hunk_id,
            "file_path": h.file_path,
            "change_type": h.change_type,
            "lines_added": h.lines_added,
            "lines_removed": h.lines_removed,
            "line_range_after_start": h.line_range_after_start,
            "line_range_after_end": h.line_range_after_end,
            "introduced_by_event_id": h.introduced_by_event_id,
            "user_modified": h.user_modified
        }))
        .collect();

    tool_success(json!({
        "session_id": session_id,
        "file_path": file_path,
        "diff_hunks": filtered
    }))
}
