//! `JudgeRuntime` — the composed judge stack wired to the pipeline (slice-15).
//!
//! The pipeline receives a `&JudgeRuntime` per rebuild. The runtime holds the
//! composed `BudgetGuard<CachedProvider<impl JudgeProvider>>` as a
//! `Box<dyn JudgeProvider>`, plus shared metrics.
//!
//! Per DEV-S15-02: each `build_for_rebuild()` call constructs a fresh
//! `BudgetGuardDyn` with a fresh budget counter. Two concurrent rebuilds each
//! get their own independent budget.

use std::path::Path;
use std::sync::Arc;

use sqlx::SqlitePool;

use crate::insight::judge::cache::CachedProvider;
use crate::insight::judge::metrics::{JudgeMetrics, MetricsSnapshot};
use crate::insight::judge::providers::{AnthropicJudge, FixtureJudge, NoopJudge};
use crate::insight::judge::{JudgeError, JudgePrompt, JudgeProvider, JudgeVerdict};

/// Which judge implementation is active — surfaced in /v1/health.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum JudgeKind {
    Noop,
    Fixture,
    Anthropic,
}

impl std::fmt::Display for JudgeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Noop => write!(f, "noop"),
            Self::Fixture => write!(f, "fixture"),
            Self::Anthropic => write!(f, "anthropic"),
        }
    }
}

/// The composed judge stack — holds a provider factory + shared metrics.
/// The factory is called once per rebuild to create a fresh `BudgetGuardDyn`.
pub struct JudgeRuntime {
    pub kind: JudgeKind,
    pub budget: usize,
    pub metrics: Arc<JudgeMetrics>,
    provider_factory: Arc<dyn Fn(Arc<JudgeMetrics>, usize) -> Box<dyn JudgeProvider> + Send + Sync>,
}

impl JudgeRuntime {
    /// Build a NoopJudge runtime (default — no LLM calls, no budget consumed).
    pub fn noop() -> Self {
        Self {
            kind: JudgeKind::Noop,
            budget: 0,
            metrics: Arc::new(JudgeMetrics::default()),
            provider_factory: Arc::new(|metrics, budget| {
                Box::new(BudgetGuardDyn {
                    inner: Box::new(NoopJudge),
                    remaining: std::sync::atomic::AtomicUsize::new(budget),
                    metrics,
                })
            }),
        }
    }

    /// Build a FixtureJudge runtime for tests/smoke.
    pub fn fixture(path: &Path, budget: usize) -> anyhow::Result<Self> {
        let judge = FixtureJudge::load(path)?;
        let arc = Arc::new(judge);
        Ok(Self {
            kind: JudgeKind::Fixture,
            budget,
            metrics: Arc::new(JudgeMetrics::default()),
            provider_factory: Arc::new(move |metrics, budget| {
                Box::new(BudgetGuardDyn {
                    inner: Box::new(FixtureJudgeAdapter(arc.clone())),
                    remaining: std::sync::atomic::AtomicUsize::new(budget),
                    metrics,
                })
            }),
        })
    }

    /// Alias for fixture() with the same budget — kept for symmetry with fixture_with_budget().
    pub fn fixture_with_budget(path: &Path, budget: usize) -> anyhow::Result<Self> {
        Self::fixture(path, budget)
    }

    /// Build an AnthropicJudge runtime (production; requires ANTHROPIC_API_KEY).
    pub fn anthropic(pool: SqlitePool, budget: usize) -> anyhow::Result<Self> {
        let judge = AnthropicJudge::from_env()?;
        let arc = Arc::new(judge);
        Ok(Self {
            kind: JudgeKind::Anthropic,
            budget,
            metrics: Arc::new(JudgeMetrics::default()),
            provider_factory: Arc::new(move |metrics, budget| {
                let cached = CachedProvider::with_metrics(
                    AnthropicAdapter(arc.clone()),
                    pool.clone(),
                    metrics.clone(),
                );
                Box::new(BudgetGuardDyn {
                    inner: Box::new(cached),
                    remaining: std::sync::atomic::AtomicUsize::new(budget),
                    metrics,
                })
            }),
        })
    }

    /// Create a fresh `BudgetGuardDyn`-wrapped provider for a single rebuild invocation.
    /// Each call returns a new guard with a fresh budget counter.
    pub fn build_for_rebuild(&self) -> Box<dyn JudgeProvider> {
        (self.provider_factory)(self.metrics.clone(), self.budget)
    }

    /// Snapshot current metrics for the health endpoint.
    pub fn metrics_snapshot(&self) -> MetricsSnapshot {
        self.metrics.snapshot()
    }
}

// --- Internal adapters so Arc<T> satisfies JudgeProvider ---

struct FixtureJudgeAdapter(Arc<FixtureJudge>);

#[async_trait::async_trait]
impl JudgeProvider for FixtureJudgeAdapter {
    async fn judge(&self, p: JudgePrompt) -> Result<JudgeVerdict, JudgeError> {
        self.0.judge(p).await
    }
    fn model_id(&self) -> &'static str {
        "fixture"
    }
    fn prompt_template_version(&self) -> &'static str {
        "fixture@v1"
    }
}

struct AnthropicAdapter(Arc<AnthropicJudge>);

#[async_trait::async_trait]
impl JudgeProvider for AnthropicAdapter {
    async fn judge(&self, p: JudgePrompt) -> Result<JudgeVerdict, JudgeError> {
        self.0.judge(p).await
    }
    fn model_id(&self) -> &'static str {
        AnthropicJudge::MODEL
    }
    fn prompt_template_version(&self) -> &'static str {
        self.0.prompt_template_version()
    }
}

/// A dynamic BudgetGuard that wraps a boxed provider. Constructed per-rebuild.
struct BudgetGuardDyn {
    inner: Box<dyn JudgeProvider>,
    remaining: std::sync::atomic::AtomicUsize,
    metrics: Arc<JudgeMetrics>,
}

#[async_trait::async_trait]
impl JudgeProvider for BudgetGuardDyn {
    async fn judge(&self, p: JudgePrompt) -> Result<JudgeVerdict, JudgeError> {
        let prev = self.remaining.fetch_update(
            std::sync::atomic::Ordering::SeqCst,
            std::sync::atomic::Ordering::SeqCst,
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
