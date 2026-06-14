-- Workflow run grouping: subagents spawned by the Workflow tool are filed under
-- <sessionId>/subagents/workflows/<runId>/agent-<id>.{jsonl,meta.json}. The runId
-- is the deterministic group key for a workflow fan-out (turn_id drifts across the
-- run as the conversation advances; OTel parent_agent_id is null for workflow
-- agents). Captured from the file path at ingest (mapping.rs / store.rs); existing
-- rows are backfilled from raw_event.source_uri by
-- repo_observed::backfill_workflow_run_id (called on serve/ingest startup) — no
-- init-db or re-ingest required.
ALTER TABLE observed_event ADD COLUMN workflow_run_id TEXT;
CREATE INDEX IF NOT EXISTS idx_obs_workflow_run
  ON observed_event(session_id, workflow_run_id) WHERE workflow_run_id IS NOT NULL;
