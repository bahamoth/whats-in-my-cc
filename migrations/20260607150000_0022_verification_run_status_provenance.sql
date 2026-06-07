-- Plan 6: Add status_provenance to verification_run.
-- Records how the command outcome was determined:
--   "measured"  — from OTLP success attribute, hook exit_code, or explicit
--                 "exit code: N" in content.
--   "estimated" — from tool-specific content failure pattern (Tier-4).
--   "unknown"   — no signal available; is_error not used for outcome.
--
-- Nullable for backward compat with existing rows (pre-Plan-6 rows will have
-- NULL until re-ingested).
ALTER TABLE verification_run ADD COLUMN status_provenance TEXT;
