-- slice-12: episode side-table
--
-- Each row represents one contiguous phase span within a session.
-- `episode_id` is deterministic: "ep_" + sha256(session_id||phase||start_event_id||end_event_id).
-- `classification_basis` and `evidence_node_ids` are JSON arrays stored as TEXT.
-- `summary` is NULL in slice-12; reserved for LLM fill-in post-MVP.

CREATE TABLE IF NOT EXISTS episode (
    episode_id              TEXT PRIMARY KEY,
    schema_version          TEXT NOT NULL DEFAULT 'episode.v1',
    session_id              TEXT NOT NULL,
    phase                   TEXT NOT NULL,
    start_event_id          TEXT NOT NULL,
    end_event_id            TEXT NOT NULL,
    started_at              TEXT NOT NULL,
    ended_at                TEXT NOT NULL,
    evidence_node_ids       TEXT NOT NULL,
    classification_basis    TEXT NOT NULL,
    confidence              REAL NOT NULL,
    summary                 TEXT,
    classifier_version      TEXT NOT NULL,
    created_at              TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_episode_session_started
    ON episode(session_id, started_at);

CREATE INDEX IF NOT EXISTS idx_episode_session_phase
    ON episode(session_id, phase);
