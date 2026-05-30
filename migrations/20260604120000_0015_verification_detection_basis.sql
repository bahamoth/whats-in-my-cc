-- Slice insight-surface-redesign #2: verification detection-basis columns.
-- Adds provenance for *how* a verification run was detected and *how* its
-- pass/fail status was derived. See design spec §6.2 (Q4).
--
--   detection_basis: "known_tool"  — Tier-1 allowlist match (high confidence)
--                    "test_keyword"— Tier-2 keyword fallback (guess)
--   status_basis:    "exit"        — status came from tool_result.is_error
--                    "piped"        — matched segment piped to a non-pager
--                                     command; exit code masked → status unknown
--
-- Backfill: existing rows predate the rewrite. They were all Tier-1 matches
-- with exit-derived status, so defaulting to 'known_tool' / 'exit' is correct
-- for historical rows; re-ingest (witmcc init-db + ingest --all) recomputes
-- them precisely under the new detector.

ALTER TABLE verification_run
    ADD COLUMN detection_basis TEXT NOT NULL DEFAULT 'known_tool';

ALTER TABLE verification_run
    ADD COLUMN status_basis TEXT NOT NULL DEFAULT 'exit';
