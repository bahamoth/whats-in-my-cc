-- Slice-14: Finding table for L1 deterministic insight extractor results.
-- finding_id is derived deterministically: "find_" + sha256(category || "\0" || session_id || "\0" || sorted_evidence_refs_join)
-- This allows INSERT OR REPLACE to safely deduplicate re-runs.

CREATE TABLE IF NOT EXISTS finding (
    finding_id          TEXT PRIMARY KEY,
    schema_version      TEXT NOT NULL DEFAULT 'finding.v1',
    session_id          TEXT NOT NULL,
    category            TEXT NOT NULL,
    severity            TEXT NOT NULL,
    confidence          REAL NOT NULL,
    summary             TEXT NOT NULL,
    evidence_refs       TEXT NOT NULL,        -- JSON array of event_id strings
    evidence_projection TEXT NOT NULL,        -- JSON object — L1-side projection
    provenance          TEXT NOT NULL,        -- JSON: { extractor, layer, judge, judge_template_version, rule_pack }
    status              TEXT NOT NULL DEFAULT 'active',  -- "active" | "pending_judge" | "discarded"
    created_at          TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_finding_session   ON finding(session_id);
CREATE INDEX IF NOT EXISTS idx_finding_category  ON finding(category);
CREATE INDEX IF NOT EXISTS idx_finding_sev_sess  ON finding(severity, session_id);
