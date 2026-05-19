-- 0001_init: slice-1 transcript schema
PRAGMA foreign_keys = ON;

CREATE TABLE ingest_run (
    run_id      TEXT PRIMARY KEY,
    started_at  TEXT NOT NULL,
    finished_at TEXT,
    status      TEXT NOT NULL,
    stats       TEXT
);

CREATE TABLE raw_event (
    raw_event_id        TEXT PRIMARY KEY,
    ingest_run_id       TEXT NOT NULL REFERENCES ingest_run(run_id),
    source_type         TEXT NOT NULL,
    source_uri          TEXT NOT NULL,
    source_line_no      INTEGER NOT NULL,
    source_byte_offset  INTEGER NOT NULL,
    payload_sha256      TEXT NOT NULL,
    payload             BLOB NOT NULL,
    parse_error         TEXT,
    captured_at         TEXT NOT NULL,
    UNIQUE(source_uri, source_line_no, payload_sha256)
);
CREATE INDEX idx_raw_event_source ON raw_event(source_uri, source_line_no);

CREATE TABLE observed_event (
    event_id                   TEXT PRIMARY KEY,
    raw_event_id               TEXT NOT NULL REFERENCES raw_event(raw_event_id),
    schema_version             TEXT NOT NULL,
    session_id                 TEXT NOT NULL,
    event_uuid                 TEXT,
    parent_uuid                TEXT,
    observed_at                TEXT NOT NULL,
    actor                      TEXT NOT NULL,
    kind                       TEXT NOT NULL,
    subkind                    TEXT,
    tool_use_id                TEXT,
    tool_name                  TEXT,
    request_id                 TEXT,
    message_id                 TEXT,
    turn_id                    TEXT,
    source_tool_assistant_uuid TEXT,
    source_tool_use_id         TEXT,
    is_sidechain               INTEGER NOT NULL DEFAULT 0,
    is_meta                    INTEGER NOT NULL DEFAULT 0,
    cwd                        TEXT,
    git_branch                 TEXT,
    user_type                  TEXT,
    entrypoint                 TEXT,
    cc_version                 TEXT,
    payload                    TEXT NOT NULL,
    trace_id                   TEXT,
    span_id                    TEXT,
    parent_span_id             TEXT,
    latency_ms                 INTEGER,
    redaction_state            TEXT,
    parser_version             TEXT NOT NULL
);
CREATE INDEX idx_obs_session_time ON observed_event(session_id, observed_at);
CREATE INDEX idx_obs_tool_use_id  ON observed_event(tool_use_id) WHERE tool_use_id IS NOT NULL;
CREATE INDEX idx_obs_event_uuid   ON observed_event(event_uuid)  WHERE event_uuid  IS NOT NULL;
CREATE INDEX idx_obs_parent_uuid  ON observed_event(parent_uuid) WHERE parent_uuid IS NOT NULL;
CREATE INDEX idx_obs_turn_id      ON observed_event(session_id, turn_id);

CREATE TABLE graph_node (
    node_id          TEXT PRIMARY KEY,
    schema_version   TEXT NOT NULL,
    session_id       TEXT NOT NULL,
    node_kind        TEXT NOT NULL,
    started_at       TEXT NOT NULL,
    ended_at         TEXT,
    merge_keys       TEXT NOT NULL,
    source_event_ids TEXT NOT NULL,
    source_uris      TEXT NOT NULL,
    payload          TEXT NOT NULL
);
CREATE INDEX idx_graph_node_session ON graph_node(session_id, started_at);
CREATE INDEX idx_graph_node_kind    ON graph_node(session_id, node_kind);

CREATE TABLE graph_edge (
    edge_id        TEXT PRIMARY KEY,
    schema_version TEXT NOT NULL,
    session_id     TEXT NOT NULL,
    from_node_id   TEXT NOT NULL REFERENCES graph_node(node_id),
    to_node_id     TEXT NOT NULL REFERENCES graph_node(node_id),
    edge_kind      TEXT NOT NULL,
    origin         TEXT NOT NULL DEFAULT 'deterministic',
    attributes     TEXT NOT NULL DEFAULT '{}'
);
CREATE INDEX idx_graph_edge_session ON graph_edge(session_id, edge_kind);
CREATE INDEX idx_graph_edge_from    ON graph_edge(from_node_id);
CREATE INDEX idx_graph_edge_to      ON graph_edge(to_node_id);
