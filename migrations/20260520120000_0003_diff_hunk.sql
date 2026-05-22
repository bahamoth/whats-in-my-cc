-- 0003_diff_hunk: slice-10a DiffHunk side-table for transcript-driven file
-- lineage. Replaces the slice-5 git-poller-attributed schema.
--
-- Each row is also represented as an observed_event (kind=diff_hunk). The
-- side-table exists to support spec-defined `file_lineage_idx` and future
-- lineage queries without scanning observed_event.payload JSON.
--
-- Attribution: a hunk is produced from a transcript `tool_result` line, so we
-- carry the producing event_id (`introduced_by_event_id`) and the source
-- tool_use_id (`introduced_by_tool_use_id`). git-derived columns from
-- slice-5 (`introduced_by_commit_sha`) are gone — slice-10a does not track
-- git commits.
--
-- `user_modified` mirrors the transcript `toolUseResult.userModified` flag so
-- M5 risky_action finding rules can read it without re-parsing payload.

CREATE TABLE IF NOT EXISTS diff_hunk (
    diff_hunk_id              TEXT PRIMARY KEY,
    schema_version            TEXT NOT NULL,
    session_id                TEXT NOT NULL,
    file_path                 TEXT NOT NULL,
    change_type               TEXT NOT NULL,
    line_range_after_start    INTEGER,
    line_range_after_end      INTEGER,
    introduced_by_event_id    TEXT NOT NULL,
    introduced_by_tool_use_id TEXT,
    patch_preview             TEXT NOT NULL,
    lines_added               INTEGER NOT NULL,
    lines_removed             INTEGER NOT NULL,
    user_modified             INTEGER NOT NULL DEFAULT 0,
    created_at                TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS file_lineage_idx
    ON diff_hunk(file_path, diff_hunk_id);
CREATE INDEX IF NOT EXISTS diff_hunk_event_idx
    ON diff_hunk(introduced_by_event_id);
CREATE INDEX IF NOT EXISTS diff_hunk_tool_use_idx
    ON diff_hunk(introduced_by_tool_use_id);
