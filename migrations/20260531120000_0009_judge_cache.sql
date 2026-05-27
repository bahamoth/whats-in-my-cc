-- Slice-15: LLM judge verdict cache.
-- cache_key = sha256(category || "\0" || model_id || "\0" || prompt_template_version || "\0" || evidence_hash)
-- Cache is cross-session: same evidence in different sessions shares a cached verdict.
-- Retention: swept by slice-19 (entries older than 30d by last_hit_at).

CREATE TABLE IF NOT EXISTS judge_verdict_cache (
    cache_key                   TEXT PRIMARY KEY,
    category                    TEXT NOT NULL,
    model_id                    TEXT NOT NULL,
    prompt_template_version     TEXT NOT NULL,
    evidence_hash               TEXT NOT NULL,
    verdict_json                TEXT NOT NULL,        -- JSON-serialised JudgeVerdict
    created_at                  TEXT NOT NULL DEFAULT (datetime('now')),
    last_hit_at                 TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_judge_cache_category ON judge_verdict_cache(category);
