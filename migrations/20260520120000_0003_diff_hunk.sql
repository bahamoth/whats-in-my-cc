-- 0003_diff_hunk: slice-5 DiffHunk side-table for file lineage queries.
-- Each row is also represented as an observed_event (kind=diff_hunk).  The
-- side-table exists to support spec-defined `file_lineage_idx` and future
-- lineage queries without scanning observed_event.payload JSON.

CREATE TABLE IF NOT EXISTS diff_hunk (
    diff_hunk_id              TEXT PRIMARY KEY,
    schema_version            TEXT NOT NULL,
    session_id                TEXT NOT NULL,
    file_path                 TEXT NOT NULL,
    change_type               TEXT NOT NULL,
    line_start_after          INTEGER,
    line_end_after            INTEGER,
    introduced_by_node_id     TEXT,
    related_observed_event_id TEXT,
    created_at                TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS file_lineage_idx
    ON diff_hunk(file_path, diff_hunk_id);
