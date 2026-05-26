-- 0005_findings: slice-11 — M5 first finding rule (`tool_failure`).
--
-- Findings are evidence-linked observations produced by insight rules over a
-- session's events + graph nodes. The schema mirrors spec §08 (data model
-- §08 Finding / Hypothesis / QualitySummary), keeping the JSON columns
-- versioned via `schema_version` for forward-compatible reshape.
--
-- Per spec "No annotation model": this table is rule-write-only. No external
-- API mutates rows; the Pull API and MCP transport surface it as read-only.

CREATE TABLE IF NOT EXISTS finding (
    finding_id       TEXT PRIMARY KEY,
    schema_version   TEXT NOT NULL,
    session_id       TEXT NOT NULL,
    category         TEXT NOT NULL,
    severity         TEXT NOT NULL,
    claim            TEXT NOT NULL,
    confidence       REAL NOT NULL,
    limitations_json TEXT NOT NULL DEFAULT '[]',
    evidence_refs_json TEXT NOT NULL DEFAULT '[]',
    generated_at     TEXT NOT NULL,
    rule_version     TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS finding_session_idx
    ON finding(session_id);
CREATE INDEX IF NOT EXISTS finding_category_idx
    ON finding(category, severity, confidence);
