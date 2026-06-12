//! Plan 4 — Detector manifest self-description tests (TDD red-lock).
//!
//! Each detector's `manifest()` must return a truthful, non-empty description
//! that matches the detector's actual code behavior. The `inputs` field must
//! reference the raw payload fields the detector actually reads.

use wimcc::insight::extractor::Detector;
use wimcc::insight::extractors::context_bloat::ContextBloat;
use wimcc::insight::extractors::final_state_mismatch::FinalStateMismatch;
use wimcc::insight::extractors::risky_action::RiskyAction;
use wimcc::insight::extractors::tool_failure::ToolFailure;

#[test]
fn tool_failure_manifest_is_self_describing() {
    let m = ToolFailure.manifest();
    assert_eq!(m.id, "tool_failure");
    assert!(!m.intent.is_empty(), "intent must be non-empty");
    assert!(
        m.inputs.iter().any(|i| i.contains("is_error")),
        "inputs must include 'is_error'; got: {:?}",
        m.inputs
    );
    assert!(!m.rationale.is_empty(), "rationale must be non-empty");
    // API-visible manifest text must not cite retired plan documents as if
    // they were live references — rationale anchors are the real fixture and
    // the spec section (doc-audit-2026-06-10 backlog).
    assert!(
        !m.intent.contains("Plan") && !m.rationale.contains("Plan"),
        "manifest text must not cite plan documents; intent={:?} rationale={:?}",
        m.intent,
        m.rationale
    );
    assert!(
        m.rationale.contains("tool_failure_v01.jsonl"),
        "rationale must anchor to the frozen real fixture; got: {:?}",
        m.rationale
    );
    assert!(!m.rule.is_empty(), "rule must be non-empty");
    assert!(!m.output.is_empty(), "output must be non-empty");
    assert!(
        m.config_keys.contains(&"retry_window"),
        "config_keys must include 'retry_window'; got: {:?}",
        m.config_keys
    );
}

#[test]
fn risky_action_manifest_is_self_describing() {
    let m = RiskyAction.manifest();
    assert_eq!(m.id, "risky_action");
    assert!(!m.intent.is_empty());
    assert!(
        m.inputs.iter().any(|i| i.contains("command")),
        "inputs must reference command field; got: {:?}",
        m.inputs
    );
    assert!(!m.rationale.is_empty());
    assert!(!m.rule.is_empty());
    // risky_action reads no cfg keys (it panics on missing pattern but has no usize_param)
    // config_keys may be empty — that is correct per code.
}

#[test]
fn context_bloat_manifest_is_self_describing() {
    let m = ContextBloat.manifest();
    assert_eq!(m.id, "context_bloat");
    assert!(!m.intent.is_empty());
    assert!(
        m.inputs.iter().any(|i| i.contains("content")),
        "inputs must reference tool_result content; got: {:?}",
        m.inputs
    );
    assert!(!m.rationale.is_empty());
    assert!(!m.rule.is_empty());
    assert!(
        m.config_keys.contains(&"threshold_bytes"),
        "config_keys must include 'threshold_bytes'; got: {:?}",
        m.config_keys
    );
    assert!(
        m.config_keys.contains(&"next_event_window"),
        "config_keys must include 'next_event_window'; got: {:?}",
        m.config_keys
    );
    assert!(
        m.config_keys.contains(&"min_overlap_stems"),
        "config_keys must include 'min_overlap_stems'; got: {:?}",
        m.config_keys
    );
}

#[test]
fn final_state_mismatch_manifest_is_self_describing() {
    let m = FinalStateMismatch.manifest();
    assert_eq!(m.id, "final_state_mismatch");
    assert!(!m.intent.is_empty());
    assert!(
        m.inputs
            .iter()
            .any(|i| i.contains("user_message") || i.contains("goal")),
        "inputs must reference user_message/goal; got: {:?}",
        m.inputs
    );
    assert!(!m.rationale.is_empty());
    assert!(!m.rule.is_empty());
}

#[test]
fn all_manifests_have_stable_ids() {
    let detectors: Vec<(&str, Box<dyn wimcc::insight::extractor::Detector>)> = vec![
        ("tool_failure", Box::new(ToolFailure)),
        ("risky_action", Box::new(RiskyAction)),
        ("context_bloat", Box::new(ContextBloat)),
        ("final_state_mismatch", Box::new(FinalStateMismatch)),
    ];
    for (expected_id, det) in detectors {
        let m = det.manifest();
        assert_eq!(
            m.id, expected_id,
            "manifest.id must match detector.id(); got {}",
            m.id
        );
        assert_eq!(m.id, det.id(), "manifest.id must equal detector.id()");
    }
}

#[test]
fn all_manifests_are_serializable() {
    let detectors: Vec<Box<dyn wimcc::insight::extractor::Detector>> = vec![
        Box::new(ToolFailure),
        Box::new(RiskyAction),
        Box::new(ContextBloat),
        Box::new(FinalStateMismatch),
    ];
    for det in detectors {
        let m = det.manifest();
        let json = serde_json::to_string(&m)
            .unwrap_or_else(|e| panic!("manifest for {} must serialize to JSON: {}", m.id, e));
        assert!(!json.is_empty());
        // Round-trip check: must parse back to an object with id field.
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["id"].as_str().unwrap(), det.id());
    }
}

// ---------------------------------------------------------------------------
// loop-foundations 2026-06-12 — metric_class (Goodhart 가드 메타데이터)
// ---------------------------------------------------------------------------

#[test]
fn every_manifest_declares_metric_class() {
    use wimcc::insight::extractors::re_read::ReRead;
    let detectors: Vec<Box<dyn wimcc::insight::extractor::Detector>> = vec![
        Box::new(ToolFailure),
        Box::new(RiskyAction),
        Box::new(ContextBloat),
        Box::new(FinalStateMismatch),
        Box::new(ReRead),
    ];
    for det in detectors {
        let m = det.manifest();
        assert!(
            m.metric_class == "process" || m.metric_class == "outcome",
            "{}: metric_class must be process|outcome, got {:?}",
            m.id,
            m.metric_class
        );
    }
}

#[test]
fn final_state_mismatch_is_outcome_class_others_process() {
    // outcome = 최종 상태(verification 등)에 결부 — 게임 난도 높음.
    // process = 행동 형태 — 지표가 목표가 되면 회피 가능. 소비자(LLM)가
    // process 지표 개선 주장에 outcome 동반 확인을 하도록 근거를 제공한다.
    assert_eq!(FinalStateMismatch.manifest().metric_class, "outcome");
    assert_eq!(ToolFailure.manifest().metric_class, "process");
    assert_eq!(RiskyAction.manifest().metric_class, "process");
    assert_eq!(ContextBloat.manifest().metric_class, "process");
}
