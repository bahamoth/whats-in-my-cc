//! Slice-15 — FixtureJudge replays recorded verdicts by (category, evidence_hash) key.

use std::path::Path;
use wimcc::insight::judge::providers::FixtureJudge;
use wimcc::insight::judge::{JudgeError, JudgePrompt};

#[tokio::test]
async fn fixture_judge_loads_and_returns_verdict() {
    let j = FixtureJudge::load(Path::new("tests/fixtures/judge/scenario_a.json")).unwrap();
    let p = JudgePrompt {
        category: "risky_action".to_string(),
        candidate_id: "cand_test".to_string(),
        evidence_projection: serde_json::json!({}),
        system_template: "placeholder".to_string(),
    };
    let verdict = j
        .judge_with_hash(p, "aaa111bbb222ccc333ddd444eee555ff")
        .await
        .unwrap();
    assert!(verdict.promote);
    assert!((verdict.confidence_l2 - 0.8).abs() < 0.001);
}

#[tokio::test]
async fn fixture_judge_returns_no_promote_for_second_entry() {
    let j = FixtureJudge::load(Path::new("tests/fixtures/judge/scenario_a.json")).unwrap();
    let p = JudgePrompt {
        category: "risky_action".to_string(),
        candidate_id: "cand_test".to_string(),
        evidence_projection: serde_json::json!({}),
        system_template: "placeholder".to_string(),
    };
    let verdict = j
        .judge_with_hash(p, "fff555eee444ddd333ccc222bbb111aaa")
        .await
        .unwrap();
    assert!(!verdict.promote);
    assert!((verdict.confidence_l2 - 0.0).abs() < 0.001);
}

#[tokio::test]
async fn fixture_judge_errors_on_unknown_key() {
    let j = FixtureJudge::load(Path::new("tests/fixtures/judge/scenario_a.json")).unwrap();
    let p = JudgePrompt {
        category: "risky_action".to_string(),
        candidate_id: "cand_unknown".to_string(),
        evidence_projection: serde_json::json!({}),
        system_template: "placeholder".to_string(),
    };
    let r = j.judge_with_hash(p, "nonexistent_hash").await;
    assert!(matches!(r, Err(JudgeError::Schema(_))));
}
