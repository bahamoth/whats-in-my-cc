//! Provider implementations for `JudgeProvider` (slice-15).

pub mod anthropic;
pub mod fixture;
pub mod noop;

pub use anthropic::AnthropicJudge;
pub use fixture::FixtureJudge;
pub use noop::NoopJudge;
