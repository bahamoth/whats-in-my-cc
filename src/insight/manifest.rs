//! Detector manifest — the LLM-readable declaration of a detector (spec §6.4).
//!
//! A detector has three layers:
//!   - **predicate** = the Rust code (the authoritative truth)
//!   - **config**    = `DetectorConfig` rule pack (TOML-tunable parameters)
//!   - **manifest**  = this struct (what an LLM reads to understand the detector)
//!
//! The manifest is read-only and exposed via `GET /v1/detectors` and MCP
//! `list_detectors`. It carries no runtime data; it is a static declaration
//! baked into each detector at compile time.
//!
//! Invariant: `manifest.id == detector.id()` and `manifest.inputs` must
//! reference the actual payload fields the predicate reads (code-verified).

/// LLM-readable self-description of a deterministic detector (spec §6.4).
///
/// All fields are `&'static str` or `Vec<&'static str>` — compiled into the
/// binary, zero heap allocation per call.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DetectorManifest {
    /// Stable detector id. Must equal `Detector::id()`.
    pub id: &'static str,

    /// What the detector detects — one sentence, human- and LLM-readable.
    pub intent: &'static str,

    /// Raw payload field paths the predicate actually reads (dot-notation).
    /// Verification: these must match the code in the detector's `detect()`.
    pub inputs: Vec<&'static str>,

    /// Pseudocode / natural-language rule describing the firing condition.
    pub rule: &'static str,

    /// Shape of the emitted signal's `facts` object.
    pub output: &'static str,

    /// `DetectorConfig` parameter keys the predicate reads via `usize_param`.
    /// Empty when the detector ignores config (e.g. no tunable threshold).
    pub config_keys: Vec<&'static str>,

    /// Evidence anchor: docs section URL fragment and/or real fixture path.
    pub rationale: &'static str,

    /// Goodhart 가드 메타데이터 (loop-foundations 2026-06-12): 이 detector의
    /// 신호가 과정 지표(`"process"` — 행동 형태, 지표가 목표가 되면 회피·게임
    /// 가능)인지 결과 지표(`"outcome"` — 최종 상태에 결부, 게임 난도 높음)인지.
    /// 분류 기준: verification/최종 상태를 읽으면 outcome. 판단이 아니라 정적
    /// 분류 선언이다 — 소비자(LLM)가 process 지표 개선 주장에 outcome 동반
    /// 확인을 하도록 근거를 제공한다.
    pub metric_class: &'static str,
}
