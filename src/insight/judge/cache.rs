//! `CachedProvider` — wraps any `JudgeProvider` with SQLite-backed caching (slice-15).
//!
//! Cache key: sha256(category || "\0" || model_id || "\0" || prompt_template_version || "\0" || evidence_hash)
//! where evidence_hash = sha256(canonical_json(evidence_projection)).
//!
//! Per DEV-S15-07: the prompt_template_version is included in the key so
//! changing the prompt automatically invalidates stale cache entries.

use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use crate::db::repo_judge_cache;
use crate::insight::judge::metrics::JudgeMetrics;
use crate::insight::judge::{JudgeError, JudgePrompt, JudgeProvider, JudgeVerdict};

/// Derive the SHA-256 of the canonical (key-sorted) JSON of an evidence projection.
/// Canonical JSON: recursively sort object keys, then serialize.
pub fn evidence_hash(proj: &serde_json::Value) -> String {
    let canon = canonical_json(proj);
    let mut h = Sha256::new();
    h.update(canon.as_bytes());
    hex::encode(h.finalize())
}

/// Recursively sort all object keys to produce canonical JSON.
fn canonical_json(v: &serde_json::Value) -> String {
    use serde_json::Value;
    match v {
        Value::Object(map) => {
            let mut sorted: Vec<(&String, &Value)> = map.iter().collect();
            sorted.sort_by(|a, b| a.0.cmp(b.0));
            let inner = sorted
                .iter()
                .map(|(k, v)| {
                    format!(
                        "{}:{}",
                        serde_json::to_string(k).unwrap(),
                        canonical_json(v)
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            format!("{{{inner}}}")
        }
        Value::Array(arr) => {
            let inner = arr.iter().map(canonical_json).collect::<Vec<_>>().join(",");
            format!("[{inner}]")
        }
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

/// Derive the full cache key for a prompt + provider combination.
/// Uses `system_template` as a proxy for the prompt template version so that
/// changing the template in the prompt invalidates the cache key.
pub fn cache_key(p: &JudgePrompt, model_id: &str, template_version: &str) -> String {
    let ehash = evidence_hash(&p.evidence_projection);
    let material = format!(
        "{}\x00{}\x00{}\x00{}",
        p.category, model_id, template_version, ehash
    );
    let mut h = Sha256::new();
    h.update(material.as_bytes());
    format!("sha256:{}", hex::encode(h.finalize()))
}

/// Wraps an inner `JudgeProvider` with a SQLite-backed cache.
/// On cache hit: return cached verdict, bump `last_hit_at`, skip inner call.
/// On cache miss: call inner, store result, return verdict.
pub struct CachedProvider<P: JudgeProvider> {
    inner: P,
    pool: SqlitePool,
    metrics: std::sync::Arc<JudgeMetrics>,
}

impl<P: JudgeProvider> CachedProvider<P> {
    pub fn new(inner: P, pool: SqlitePool) -> Self {
        Self {
            inner,
            pool,
            metrics: std::sync::Arc::new(JudgeMetrics::default()),
        }
    }

    pub fn with_metrics(
        inner: P,
        pool: SqlitePool,
        metrics: std::sync::Arc<JudgeMetrics>,
    ) -> Self {
        Self {
            inner,
            pool,
            metrics,
        }
    }
}

#[async_trait::async_trait]
impl<P: JudgeProvider> JudgeProvider for CachedProvider<P> {
    async fn judge(&self, p: JudgePrompt) -> Result<JudgeVerdict, JudgeError> {
        let key = cache_key(&p, self.inner.model_id(), self.inner.prompt_template_version());
        let ehash = evidence_hash(&p.evidence_projection);

        if let Some(row) = repo_judge_cache::get(&self.pool, &key)
            .await
            .map_err(|e| JudgeError::Transport(e.to_string()))?
        {
            self.metrics.cache_hit();
            let _ = repo_judge_cache::touch(&self.pool, &key).await;
            return serde_json::from_str::<JudgeVerdict>(&row.verdict_json)
                .map_err(|e| JudgeError::Schema(format!("cache verdict parse: {e}")));
        }

        self.metrics.cache_miss();
        let verdict = self.inner.judge(p.clone()).await?;

        let verdict_json = serde_json::to_string(&verdict)
            .map_err(|e| JudgeError::Schema(e.to_string()))?;

        let _ = repo_judge_cache::put(
            &self.pool,
            &key,
            &p.category,
            self.inner.model_id(),
            self.inner.prompt_template_version(),
            &ehash,
            &verdict_json,
        )
        .await;

        Ok(verdict)
    }

    fn model_id(&self) -> &'static str {
        self.inner.model_id()
    }
    fn prompt_template_version(&self) -> &'static str {
        self.inner.prompt_template_version()
    }
}
