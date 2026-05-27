-- Slice-19: Audit table.
-- Records security-relevant events: retention sweeps, token rotations, etc.
CREATE TABLE IF NOT EXISTS audit (
    audit_id    TEXT PRIMARY KEY,                  -- "aud_" + ulid
    event       TEXT NOT NULL,                     -- "retention.deleted" | "rotate.token" | "api.accessed" | "mcp.connected"
    actor       TEXT,                              -- "owner_or_local_client" | "retention_sweep" etc.
    payload     TEXT NOT NULL DEFAULT '{}',        -- JSON details
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_audit_created ON audit(created_at);
