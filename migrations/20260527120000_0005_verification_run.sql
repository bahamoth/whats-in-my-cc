-- Slice-11: VerificationRun side-table.
-- Tracks verification activity (test runs, builds, lint checks) observed from
-- Bash tool calls, hook events, and OTel spans.
--
-- parser_version: "verification_run@v1"
-- schema_version: "verification_run.v1"
--
-- DEV-S11-04: This is a side-table (not a new EventKind) because synthesis of
-- an ObservedEvent row would require either a synthetic raw_event (wrong) or
-- re-ingesting the raw payload (wasteful). The pattern mirrors diff_hunk.

CREATE TABLE IF NOT EXISTS verification_run (
    verification_run_id   TEXT PRIMARY KEY,
    -- "vr_" + sha256(session_id || trigger_event_id || started_at)
    schema_version        TEXT NOT NULL DEFAULT 'verification_run.v1',
    session_id            TEXT NOT NULL,
    source                TEXT NOT NULL,
    -- "bash" | "hook" | "otel"
    command               TEXT NOT NULL,
    -- the matched command string
    command_kind          TEXT NOT NULL,
    -- "test_suite_js" | "test_suite_rust" | "build_check" | "build" | "lint" | "format_check" | "test_suite_py" | "test_suite_go" | "test_suite_java"
    trigger_event_id      TEXT NOT NULL,
    -- observed_event.event_id that triggered this row
    trigger_tool_use_id   TEXT,
    -- nullable for the otel branch
    status                TEXT NOT NULL,
    -- "passed" | "failed" | "unknown"
    started_at            TEXT NOT NULL,
    -- ISO 8601 UTC
    ended_at              TEXT,
    -- ISO 8601 UTC; null if not derivable
    exit_code             INTEGER,
    -- bash branch only
    failure_summary       TEXT,
    -- first 512 bytes of stderr or otel status_message
    raw_event_id          TEXT NOT NULL,
    -- FK-ish into raw_event
    parser_version        TEXT NOT NULL,
    created_at            TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_verification_run_session_started
    ON verification_run(session_id, started_at);

CREATE INDEX IF NOT EXISTS idx_verification_run_trigger
    ON verification_run(trigger_event_id);
