//! `judge_verdict_cache` side-table repo (slice-15).
//!
//! Cache key is derived externally (see `insight::judge::cache`).
//! The repo does get/put/touch/sweep operations using non-macro sqlx queries
//! (consistent with the rest of this codebase — no offline mode required).

use sqlx::{Row, SqlitePool};

use crate::error::Result;

/// Stored cache row shape (returned by get).
#[derive(Debug, Clone)]
pub struct JudgeCacheRow {
    pub cache_key: String,
    pub category: String,
    pub model_id: String,
    pub prompt_template_version: String,
    pub evidence_hash: String,
    pub verdict_json: String,
    pub created_at: String,
    pub last_hit_at: String,
}

/// Look up a cached verdict by key. Returns `None` if missing.
pub async fn get(pool: &SqlitePool, cache_key: &str) -> Result<Option<JudgeCacheRow>> {
    let row = sqlx::query(
        "SELECT cache_key, category, model_id, prompt_template_version, \
         evidence_hash, verdict_json, created_at, last_hit_at \
         FROM judge_verdict_cache WHERE cache_key = ?",
    )
    .bind(cache_key)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| JudgeCacheRow {
        cache_key: r.get("cache_key"),
        category: r.get("category"),
        model_id: r.get("model_id"),
        prompt_template_version: r.get("prompt_template_version"),
        evidence_hash: r.get("evidence_hash"),
        verdict_json: r.get("verdict_json"),
        created_at: r.get("created_at"),
        last_hit_at: r.get("last_hit_at"),
    }))
}

/// Insert or replace a cache entry.
pub async fn put(
    pool: &SqlitePool,
    cache_key: &str,
    category: &str,
    model_id: &str,
    prompt_template_version: &str,
    evidence_hash: &str,
    verdict_json: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT OR REPLACE INTO judge_verdict_cache \
         (cache_key, category, model_id, prompt_template_version, evidence_hash, verdict_json) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(cache_key)
    .bind(category)
    .bind(model_id)
    .bind(prompt_template_version)
    .bind(evidence_hash)
    .bind(verdict_json)
    .execute(pool)
    .await?;
    Ok(())
}

/// Update `last_hit_at` to now for LRU-style tracking.
pub async fn touch(pool: &SqlitePool, cache_key: &str) -> Result<()> {
    sqlx::query(
        "UPDATE judge_verdict_cache SET last_hit_at = datetime('now') WHERE cache_key = ?",
    )
    .bind(cache_key)
    .execute(pool)
    .await?;
    Ok(())
}

/// Delete entries not accessed since `older_than_days` days ago (pass as negative, e.g. -30).
/// Returns the number of deleted rows.
pub async fn sweep_older_than(pool: &SqlitePool, older_than_days: i64) -> Result<u64> {
    let param = format!("{older_than_days} days");
    let res = sqlx::query(
        "DELETE FROM judge_verdict_cache WHERE last_hit_at < datetime('now', ?)",
    )
    .bind(param)
    .execute(pool)
    .await?;
    Ok(res.rows_affected())
}
