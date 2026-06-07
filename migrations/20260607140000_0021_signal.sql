-- Plan 1: finding → signal. severity/confidence(판단)를 제거하고 facts(사실)만 남긴다.
-- finding 테이블 폐기(신규+폐기, spec §10.2).
CREATE TABLE IF NOT EXISTS signal (
    signal_id           TEXT PRIMARY KEY,
    schema_version      TEXT NOT NULL DEFAULT 'signal.v1',
    session_id          TEXT NOT NULL,
    detector            TEXT NOT NULL,
    subkind             TEXT,
    summary             TEXT NOT NULL,
    evidence_refs       TEXT NOT NULL,
    facts               TEXT NOT NULL,
    provenance          TEXT NOT NULL,
    created_at          TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_signal_session  ON signal(session_id);
CREATE INDEX IF NOT EXISTS idx_signal_detector ON signal(detector);
CREATE INDEX IF NOT EXISTS idx_signal_sess_det ON signal(session_id, detector);

DROP TABLE IF EXISTS finding;
