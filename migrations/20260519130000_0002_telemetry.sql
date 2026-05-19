-- 0002_telemetry: slice-3 OTel span lookup index
CREATE INDEX IF NOT EXISTS idx_obs_trace_span
  ON observed_event(trace_id, span_id)
  WHERE trace_id IS NOT NULL;
