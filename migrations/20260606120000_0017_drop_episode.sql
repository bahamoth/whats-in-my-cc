-- Remove the episode side-table. The episode/phase classification system was
-- removed: the message view's activity-run fold replaces it and needs no
-- persisted classification. See
-- docs/superpowers/specs/2026-05-31-episode-phase-removal-design.md.
DROP TABLE IF EXISTS episode;
