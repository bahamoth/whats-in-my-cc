-- Add agent_id to observed_event for subagent attribution on sidechain events.
-- Source: transcript record `agentId` (present only in subagent jsonl files).
-- Lets the re_read detector scope by individual subagent instead of lumping all
-- sidechain reads together. Existing rows are backfilled from the raw payload by
-- repo_observed::backfill_agent_id (called on serve startup) — no init-db needed.
-- New ingests populate agent_id via mapping.rs directly. Dogfooding 2026-06-11.
ALTER TABLE observed_event ADD COLUMN agent_id TEXT;
