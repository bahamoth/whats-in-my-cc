//! Slice-15 — CachedProvider serves from DB cache on second call; misses call inner.

use sqlx::sqlite::SqlitePoolOptions;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use witmcc::db::migrate;
use witmcc::insight::judge::cache::CachedProvider;
use witmcc::insight::judge::{JudgeError, JudgePrompt, JudgeProvider, JudgeVerdict};

/// Scripted judge: returns verdicts from a pre-built list; panics when list is exhausted.
struct ScriptedJudge {
    verdicts: std::sync::Mutex<Vec<JudgeVerdict>>,
    call_count: Arc<AtomicU32>,
}

impl ScriptedJudge {
    fn new(verdicts: Vec<JudgeVerdict>) -> Self {
        Self {
            verdicts: std::sync::Mutex::new(verdicts),
            call_count: Arc::new(AtomicU32::new(0)),
        }
    }
}

#[async_trait::async_trait]
impl JudgeProvider for ScriptedJudge {
    async fn judge(&self, _p: JudgePrompt) -> Result<JudgeVerdict, JudgeError> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        let mut v = self.verdicts.lock().unwrap();
        if v.is_empty() {
            panic!("ScriptedJudge exhausted — unexpected extra call");
        }
        Ok(v.remove(0))
    }
    fn model_id(&self) -> &'static str {
        "scripted"
    }
    fn prompt_template_version(&self) -> &'static str {
        "v_test"
    }
}

fn ok_verdict(confidence: f32) -> JudgeVerdict {
    JudgeVerdict {
        promote: true,
        confidence_l2: confidence,
        reason: "scripted".to_string(),
        mismatch_summary: None,
    }
}

fn synth_prompt() -> JudgePrompt {
    JudgePrompt {
        category: "risky_action".to_string(),
        candidate_id: "cand_001".to_string(),
        evidence_projection: serde_json::json!({"cmd": "rm -rf /tmp/x"}),
        system_template: "judge@v1".to_string(),
    }
}

async fn mem_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&pool)
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    pool
}

#[tokio::test]
async fn cached_provider_calls_inner_once_then_serves_cache() {
    let pool = mem_pool().await;
    let inner = ScriptedJudge::new(vec![ok_verdict(0.8)]);
    let calls = inner.call_count.clone();
    let prov = CachedProvider::new(inner, pool);
    let p = synth_prompt();

    let v1 = prov.judge(p.clone()).await.unwrap();
    let v2 = prov.judge(p.clone()).await.unwrap();

    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "inner must be called exactly once"
    );
    assert!((v1.confidence_l2 - 0.8).abs() < 0.001);
    assert!((v2.confidence_l2 - 0.8).abs() < 0.001);
}

#[tokio::test]
async fn cache_key_differs_when_template_version_differs() {
    let pool = mem_pool().await;
    let inner = ScriptedJudge::new(vec![ok_verdict(0.7), ok_verdict(0.6)]);
    let calls = inner.call_count.clone();
    let prov = CachedProvider::new(inner, pool);

    let mut p1 = synth_prompt();
    p1.system_template = "judge@v1".to_string();
    let mut p2 = synth_prompt();
    p2.system_template = "judge@v2".to_string(); // different template → different key

    prov.judge(p1).await.unwrap();
    prov.judge(p2).await.unwrap();

    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "different template = cache miss = 2 calls"
    );
}
