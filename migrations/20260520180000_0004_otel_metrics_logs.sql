-- Slice-6: indexes for OTel metrics + logs ObservedEvent rows and raw_event source_type lookup.
-- No DDL changes to existing tables — facets ride inside observed_event.payload (TEXT JSON).
-- Side-table for diff_hunk (slice-5) is unchanged.

CREATE INDEX IF NOT EXISTS idx_obs_metric_instrument
  ON observed_event(json_extract(payload, '$.instrument_name'))
  WHERE kind = 'metric_sample';

CREATE INDEX IF NOT EXISTS idx_obs_log_event_name
  ON observed_event(json_extract(payload, '$.event_name'))
  WHERE kind = 'log_record';

-- raw_event has no session_id column (session attribution lives only on observed_event).
-- /v1/health/sources groups by source_type, so a single-column index is enough.
CREATE INDEX IF NOT EXISTS idx_raw_source_type
  ON raw_event(source_type);
