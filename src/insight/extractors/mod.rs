//! Detector implementations, one module per category. All are deterministic:
//! every emitted `SignalCandidate` promotes directly to a `signal` row (no
//! judge, no pending queue, no confidence floor).
//!
//! Slice-14:
//!   - `tool_failure`: is_error=true tool_result (facts: retried/tool_name/...).
//!
//! Slice-16:
//!   - `risky_action`: destructive Bash command or user_modified diff_hunk.
//!   - `context_bloat`: large tool_result not reused in subsequent turn.
//!   - `final_state_mismatch`: user goal not corroborated in final state.

pub mod context_bloat;
pub mod final_state_mismatch;
pub mod re_read;
pub mod risky_action;
pub mod tool_failure;

/// Test-only detector that always emits one synthetic signal candidate.
/// Only available when the `test-helpers` feature is enabled — never compiled
/// into production binaries.
#[cfg(feature = "test-helpers")]
pub mod noop_test {
    use crate::insight::config::DetectorConfig;
    use crate::insight::extractor::Detector;
    use crate::insight::manifest::DetectorManifest;
    use crate::insight::types::SignalCandidate;
    use crate::insight::view::SessionInsightView;

    pub struct NoopTestExtractor;

    impl Detector for NoopTestExtractor {
        fn id(&self) -> &'static str {
            "noop_test"
        }

        fn manifest(&self) -> DetectorManifest {
            DetectorManifest {
                id: "noop_test",
                intent: "테스트 전용 noop detector — 항상 synthetic signal 1건을 발화한다.",
                inputs: vec![],
                rule: "항상 발화(test-helpers feature 전용).",
                output: "{synthetic: true}",
                config_keys: vec![],
                rationale: "test helper only — no real detection logic",
            }
        }

        fn detect(
            &self,
            _view: &SessionInsightView<'_>,
            _cfg: &DetectorConfig,
        ) -> Vec<SignalCandidate> {
            vec![SignalCandidate {
                detector: "noop_test",
                subkind: None,
                summary: "noop_test: synthetic candidate for signal testing".to_string(),
                evidence_refs: vec!["ev_000".to_string()],
                facts: serde_json::json!({"synthetic": true}),
            }]
        }
    }
}
