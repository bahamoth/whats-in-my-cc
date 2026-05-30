-- Slice insight-surface-redesign #3 (tool_failure reframe, spec §6.3):
-- additive `subkind` column classifying a finding's sub-type. For tool_failure
-- this carries the failure class:
--   'user_visible'        — a genuine user-facing failure (severity high)
--   'internal_retry'      — internal agent auto-retry, e.g. StructuredOutput
--                           schema-retry cycles (severity info, never headlined)
--   'benign_nonzero_exit' — grep no-match / Read file-not-found (severity info)
-- NULL for findings of other categories that do not classify (back-compat).
-- Additive only — no data loss; mirrors slice-13 additive columns on graph_edge.

ALTER TABLE finding ADD COLUMN subkind TEXT;

CREATE INDEX IF NOT EXISTS idx_finding_subkind_session
    ON finding(session_id, category, subkind);
