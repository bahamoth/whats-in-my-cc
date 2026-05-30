-- Slice insight-surface-redesign #1: usage_facet side-table.
-- One row per assistant API turn (keyed by raw_event_id). Stores token usage
-- parsed from the raw transcript line's message.usage (which is NOT present in
-- observed_event.payload — only `model` is). Side-table, not a new EventKind,
-- mirroring verification_run / diff_hunk.
--
-- parser_version: "usage_facet@v1"
-- schema_version: "usage_facet.v1"

CREATE TABLE IF NOT EXISTS usage_facet (
    raw_event_id                 TEXT PRIMARY KEY,
    -- one assistant API turn = one raw transcript line = one usage object
    schema_version               TEXT NOT NULL DEFAULT 'usage_facet.v1',
    session_id                   TEXT NOT NULL,
    model                        TEXT,
    input_tokens                 INTEGER NOT NULL DEFAULT 0,
    cache_creation_input_tokens  INTEGER NOT NULL DEFAULT 0,
    cache_read_input_tokens      INTEGER NOT NULL DEFAULT 0,
    output_tokens                INTEGER NOT NULL DEFAULT 0,
    observed_at                  TEXT NOT NULL,
    -- ISO 8601 UTC of the earliest content-block event of this message
    parser_version               TEXT NOT NULL,
    created_at                   TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_usage_facet_session_observed
    ON usage_facet(session_id, observed_at);
