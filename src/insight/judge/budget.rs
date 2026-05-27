//! `BudgetGuard` — limits judge calls per rebuild invocation (slice-15).
//!
//! Budget is per-invocation: a new `BudgetGuard` is constructed in each
//! `run_extractors_with_runtime` call. Per DEV-S15-02: two concurrent rebuilds
//! each get their own budget.
//!
//! Composition: `BudgetGuard<CachedProvider<impl JudgeProvider>>`.
//! Cache wraps the network; budget wraps the cache.
//! This means cache hits do NOT consume budget (the budget counts real API calls).

use std::sync::atomic::{AtomicUsize, Ordering};

use crate::insight::judge::metrics::JudgeMetrics;
use crate::insight::judge::{JudgeError, JudgePrompt, JudgeProvider, JudgeVerdict};

/// Limits the number of judge calls in a single rebuild pass.
pub struct BudgetGuard<P: JudgeProvider> {
    inner: P,
    remaining: AtomicUsize,
    metrics: std::sync::Arc<JudgeMetrics>,
}

impl<P: JudgeProvider> BudgetGuard<P> {
    pub fn new(inner: P, budget: usize) -> Self {
        Self {
            inner,
            remaining: AtomicUsize::new(budget),
            metrics: std::sync::Arc::new(JudgeMetrics::default()),
        }
    }

    pub fn with_metrics(inner: P, budget: usize, metrics: std::sync::Arc<JudgeMetrics>) -> Self {
        Self {
            inner,
            remaining: AtomicUsize::new(budget),
            metrics,
        }
    }

    pub fn remaining(&self) -> usize {
        self.remaining.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl<P: JudgeProvider> JudgeProvider for BudgetGuard<P> {
    async fn judge(&self, p: JudgePrompt) -> Result<JudgeVerdict, JudgeError> {
        // Try to decrement budget atomically.
        let prev = self.remaining.fetch_update(
            Ordering::SeqCst,
            Ordering::SeqCst,
            |r| r.checked_sub(1),
        );
        if prev.is_err() {
            self.metrics.budget_exhaustion();
            return Err(JudgeError::BudgetExhausted);
        }
        self.metrics.call();
        self.inner.judge(p).await
    }

    fn model_id(&self) -> &'static str {
        self.inner.model_id()
    }
    fn prompt_template_version(&self) -> &'static str {
        self.inner.prompt_template_version()
    }
}
