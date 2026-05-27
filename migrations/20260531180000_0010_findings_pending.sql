-- Slice-15: Candidates that need judge evaluation but couldn't be judged yet
-- (budget exhausted, judge disabled, transport error). Drained on next rebuild
-- that has budget. Monotone progress guarantee: candidates are never silently dropped.

CREATE TABLE IF NOT EXISTS findings_pending_judge (
    candidate_id        TEXT PRIMARY KEY,                  -- == finding_id derivation key
    schema_version      TEXT NOT NULL DEFAULT 'pending_finding.v1',
    session_id          TEXT NOT NULL,
    category            TEXT NOT NULL,
    confidence_l1       REAL NOT NULL,
    evidence_refs       TEXT NOT NULL,                    -- JSON array of event_id strings
    evidence_projection TEXT NOT NULL,                    -- JSON object — projection for judge
    queued_at           TEXT NOT NULL DEFAULT (datetime('now')),
    last_attempt_at     TEXT,
    attempts            INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_pending_session ON findings_pending_judge(session_id);
