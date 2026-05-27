//! Core types for the L2 judge layer (slice-15).

/// Input to the judge: a structured prompt with category, candidate id,
/// evidence projection (compact JSON), and the versioned system template string.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct JudgePrompt {
    /// Stable category id (e.g. "risky_action").
    pub category: String,
    /// Derivation key for the finding (== finding_id without prefix).
    pub candidate_id: String,
    /// Compact JSON evidence for this candidate — what the judge sees.
    pub evidence_projection: serde_json::Value,
    /// Versioned system prompt template string.
    pub system_template: String,
}

/// The judge's structured verdict.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct JudgeVerdict {
    /// `true` → promote to Finding; `false` → discard candidate.
    pub promote: bool,
    /// Judge confidence in range [0.0, 1.0]. 0.0 when `promote == false`.
    pub confidence_l2: f32,
    /// Short one-sentence reason referencing at least one evidence field name.
    pub reason: String,
    /// Populated only for `final_state_mismatch` when `promote == true`.
    pub mismatch_summary: Option<String>,
}
