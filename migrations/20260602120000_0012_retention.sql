-- Slice-19: Retention tombstone table.
-- When a resource is deleted by the retention sweep, its ID is recorded here
-- so Pull API can distinguish "never existed" (404) from "expired" (410 Gone).
CREATE TABLE IF NOT EXISTS retention_tombstone (
    resource_id     TEXT PRIMARY KEY,
    resource_kind   TEXT NOT NULL,   -- "raw_event" | "observed_event" | "graph_node" | "finding" | "audit" | "judge_cache"
    deleted_at      TEXT NOT NULL DEFAULT (datetime('now')),
    reason          TEXT NOT NULL DEFAULT 'retention'
);
