-- 0020: add missing index on request_id for correlated telemetry lookups.
-- tool_use_id already has idx_obs_tool_use_id (from 0001).
CREATE INDEX IF NOT EXISTS idx_obs_request_id
    ON observed_event(request_id) WHERE request_id IS NOT NULL;
