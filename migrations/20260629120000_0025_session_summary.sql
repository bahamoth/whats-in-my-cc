-- perf-2026-06-29: materialize transcript-derived identity facets per session
-- so GET /v1/sessions no longer re-scans + json_extract over the whole
-- observed_event table on every request. The list previously ran 4 grouped
-- json_extract passes (project/model/slug/preview) over the IN-list of returned
-- sessions (~0.7s cold on the dogfood DB, 432 sessions / 291k events); these
-- facets are transcript-derived and only change on transcript ingest, so we
-- compute them once in recompute_session and read them back from this table.
--
-- totals (MIN/MAX/COUNT) and by_kind stay LIVE queries on observed_event:
-- totals already rides the covering index idx_obs_session_time, and the
-- by_kind covering index below lets its GROUP BY (session_id, kind) answer from
-- the index without touching table rows. Keeping both live means they stay
-- accurate even for OTLP-only sessions that never reach recompute_session
-- (the facets here are null for such sessions, which is correct — they have no
-- transcript model/slug/preview).
--
-- Existing rows are backfilled by repo_observed::backfill_session_summary
-- (serve/ingest startup) — no init-db / re-ingest required. New/updated
-- sessions refresh via recompute_session (transcript ingest path).
CREATE TABLE IF NOT EXISTS session_summary (
    session_id                 TEXT PRIMARY KEY,
    project                    TEXT,
    model                      TEXT,
    slug                       TEXT,
    first_user_message_preview TEXT,
    updated_at                 TEXT NOT NULL
);

-- Covering index for the live by_kind facet: SELECT session_id, kind, COUNT(*)
-- GROUP BY session_id, kind. Previously fell back to idx_obs_turn_id and
-- touched table rows (~0.26s); with this index the group-by is answered from
-- the index alone.
CREATE INDEX IF NOT EXISTS idx_obs_session_kind
  ON observed_event(session_id, kind);
