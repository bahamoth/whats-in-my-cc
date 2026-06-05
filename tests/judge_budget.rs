//! Slice-15 — BudgetGuard exhausts after N calls and returns BudgetExhausted.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use wimcc::insight::judge::budget::BudgetGuard;
use wimcc::insight::judge::{JudgeError, JudgePrompt, JudgeProvider, JudgeVerdict};

struct InfiniteJudge {
    call_count: Arc<AtomicU32>,
}

impl InfiniteJudge {
    fn new() -> (Self, Arc<AtomicU32>) {
        let c = Arc::new(AtomicU32::new(0));
        (
            Self {
                call_count: c.clone(),
            },
            c,
        )
    }
}

#[async_trait::async_trait]
impl JudgeProvider for InfiniteJudge {
    async fn judge(&self, _p: JudgePrompt) -> Result<JudgeVerdict, JudgeError> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        Ok(JudgeVerdict {
            promote: true,
            confidence_l2: 0.5,
            reason: "ok".to_string(),
            mismatch_summary: None,
        })
    }
    fn model_id(&self) -> &'static str {
        "infinite"
    }
    fn prompt_template_version(&self) -> &'static str {
        "v_test"
    }
}

fn p() -> JudgePrompt {
    JudgePrompt {
        category: "risky_action".to_string(),
        candidate_id: "c".to_string(),
        evidence_projection: serde_json::json!({}),
        system_template: "s".to_string(),
    }
}

#[tokio::test]
async fn budget_guard_exhausts_after_budget_calls() {
    let (inner, calls) = InfiniteJudge::new();
    let guard = BudgetGuard::new(inner, 3);
    assert!(guard.judge(p()).await.is_ok());
    assert!(guard.judge(p()).await.is_ok());
    assert!(guard.judge(p()).await.is_ok());
    let r = guard.judge(p()).await;
    assert!(
        matches!(r, Err(JudgeError::BudgetExhausted)),
        "expected BudgetExhausted, got {:?}",
        r
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        3,
        "inner called exactly 3 times"
    );
}

#[tokio::test]
async fn budget_guard_zero_budget_exhausts_immediately() {
    let (inner, _) = InfiniteJudge::new();
    let guard = BudgetGuard::new(inner, 0);
    let r = guard.judge(p()).await;
    assert!(matches!(r, Err(JudgeError::BudgetExhausted)));
}

#[tokio::test]
async fn budget_guard_delegates_model_id() {
    let (inner, _) = InfiniteJudge::new();
    let guard = BudgetGuard::new(inner, 5);
    assert_eq!(guard.model_id(), "infinite");
}
