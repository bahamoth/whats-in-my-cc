//! Slice-15 — NoopJudge must return JudgeError::Disabled.

use witmcc::insight::judge::providers::NoopJudge;
use witmcc::insight::judge::{JudgeError, JudgePrompt, JudgeProvider};

fn synth_prompt() -> JudgePrompt {
    JudgePrompt {
        category: "risky_action".to_string(),
        candidate_id: "cand_001".to_string(),
        evidence_projection: serde_json::json!({"test": true}),
        system_template: "placeholder".to_string(),
    }
}

#[tokio::test]
async fn noop_judge_returns_disabled_error() {
    let j = NoopJudge;
    let r = j.judge(synth_prompt()).await;
    assert!(
        matches!(r, Err(JudgeError::Disabled)),
        "expected Disabled, got {:?}",
        r
    );
}

#[test]
fn noop_model_id_is_noop() {
    let j = NoopJudge;
    assert_eq!(j.model_id(), "noop");
}

#[test]
fn noop_prompt_template_version_is_noop() {
    let j = NoopJudge;
    assert_eq!(j.prompt_template_version(), "noop");
}
