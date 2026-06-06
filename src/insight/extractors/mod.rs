//! Extractor implementations, one module per category.
//! Slice-14: `tool_failure` L1 deterministic extractor:
//!   - `tool_failure`: is_error=true with no compensating retry within 5 events.
//!
//! Slice-15: `noop_test` extractor added under `cfg(test)` — exercises the full
//! L2 code path (pending queue, cache, budget) without introducing a production
//! finding category.
//!
//! Slice-16: three L1+L2 extractors (always-judge):
//!   - `risky_action`: destructive Bash command or user_modified diff_hunk.
//!   - `context_bloat`: large tool_result not reused in subsequent turn.
//!   - `final_state_mismatch`: user goal not corroborated in final state.

pub mod context_bloat;
pub mod final_state_mismatch;
pub mod risky_action;
pub mod tool_failure;

/// Test-only extractor that always emits one synthetic candidate.
/// Only available when the `test-helpers` feature is enabled — never compiled
/// into production binaries.
#[cfg(feature = "test-helpers")]
pub mod noop_test {
    use crate::insight::extractor::InsightExtractor;
    use crate::insight::types::FindingCandidate;
    use crate::insight::view::SessionInsightView;

    pub struct NoopTestExtractor;

    impl InsightExtractor for NoopTestExtractor {
        fn category(&self) -> &'static str {
            "noop_test"
        }
        fn floor(&self) -> f32 {
            0.5
        }
        fn extract(&self, _view: &SessionInsightView<'_>) -> Vec<FindingCandidate> {
            vec![FindingCandidate {
                category: "noop_test",
                subkind: None,
                confidence_l1: 0.9,
                severity: "low",
                summary: "noop_test: synthetic candidate for L1 testing".to_string(),
                evidence_refs: vec!["ev_000".to_string()],
                evidence_projection: serde_json::json!({"synthetic": true}),
            }]
        }
    }
}
