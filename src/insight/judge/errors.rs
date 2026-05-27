//! Error variants for the L2 judge layer (slice-15).

/// Errors that can be returned by a `JudgeProvider::judge()` call.
#[derive(Debug)]
pub enum JudgeError {
    /// Network failure or 5xx from the API.
    Transport(String),
    /// Model returned malformed JSON or unexpected schema.
    Schema(String),
    /// Request timed out.
    Timeout,
    /// Budget guard exhausted the per-rebuild call budget.
    BudgetExhausted,
    /// Judge is explicitly disabled (NoopJudge).
    Disabled,
}

impl std::fmt::Display for JudgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transport(s) => write!(f, "judge transport error: {s}"),
            Self::Schema(s) => write!(f, "judge schema error: {s}"),
            Self::Timeout => write!(f, "judge timeout"),
            Self::BudgetExhausted => write!(f, "judge budget exhausted"),
            Self::Disabled => write!(f, "judge disabled"),
        }
    }
}

impl std::error::Error for JudgeError {}
