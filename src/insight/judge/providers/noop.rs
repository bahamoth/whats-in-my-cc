//! `NoopJudge` — always returns `Err(JudgeError::Disabled)` (slice-15).
//!
//! Used when the user has not opted into LLM judgment.
//! Returning `Err(Disabled)` (not `Ok(promote=false)`) so the pipeline
//! distinguishes "judge disabled" from "judge ran and said no" — per DEV-S15-06.

use crate::insight::judge::{JudgeError, JudgePrompt, JudgeProvider, JudgeVerdict};

pub struct NoopJudge;

#[async_trait::async_trait]
impl JudgeProvider for NoopJudge {
    async fn judge(&self, _p: JudgePrompt) -> Result<JudgeVerdict, JudgeError> {
        Err(JudgeError::Disabled)
    }

    fn model_id(&self) -> &'static str {
        "noop"
    }

    fn prompt_template_version(&self) -> &'static str {
        "noop"
    }
}
