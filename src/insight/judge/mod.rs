//! L2 LLM judge infrastructure (slice-15).
//!
//! Architecture: `BudgetGuard<CachedProvider<impl JudgeProvider>>`.
//! Default is `NoopJudge` — L2 is off by default.
//! Opt-in via `--judge anthropic` or `--judge fixture` CLI flags.

pub mod budget;
pub mod cache;
pub mod errors;
pub mod metrics;
pub mod providers;
pub mod runtime;
pub mod types;

pub use errors::JudgeError;
pub use types::{JudgePrompt, JudgeVerdict};

/// Contract for all judge implementations.
/// Implementations: `NoopJudge`, `FixtureJudge`, `AnthropicJudge`.
/// Wrapped by `CachedProvider` then `BudgetGuard` in production.
#[async_trait::async_trait]
pub trait JudgeProvider: Send + Sync {
    /// Attempt to judge the candidate. On error, the pipeline queues the
    /// candidate to `findings_pending_judge`.
    async fn judge(&self, prompt: JudgePrompt) -> Result<JudgeVerdict, JudgeError>;

    /// Stable, version-suffixed model name. Surfaced in Finding.provenance.
    fn model_id(&self) -> &'static str;

    /// Version of the prompt template. Part of the cache key — changing the
    /// template automatically invalidates prior cache entries for this judge.
    fn prompt_template_version(&self) -> &'static str;
}
