//! Deterministic signal detector pipeline (Plan 1: finding → signal).
//!
//! Every `SignalCandidate` a detector emits is promoted directly to a `signal`
//! row. No judge, no confidence floor, no pending queue — detectors emit facts,
//! not judgments (spec §6.3).
//!
//! `run_detectors` is idempotent at the *session* level: each call loads the full
//! session view and rebuilds the complete signal set. Two mechanisms keep the
//! stored set in sync with the latest pass:
//!   1. Stable `signal_id` — derived from `dedup_key` when a detector provides one
//!      (aggregating detectors like re_read, keyed by file_path), else from
//!      `evidence_refs`. `INSERT OR REPLACE` then updates one row as evidence grows.
//!   2. Reconcile — after inserting a detector's current-pass signals, delete any
//!      stored signal for that (session, detector) absent from the pass. This
//!      removes stale rows that a *changed* `signal_id` left orphaned — the
//!      dogfooding regression (2026-06-11) where re_read evidence grew across
//!      re-ingests and spawned a fresh row each time (154 rows / 53 files).
//!
//! Called directly from each ingest path (transcript / OTel traces·logs·metrics
//! / hook) once observed events for a session have been committed.

use sqlx::SqlitePool;

use crate::db::repo_signal::{self, SignalRow};
use crate::error::Result;
use crate::ids::derive_signal_id;
use crate::insight::config::DetectorConfig;
use crate::insight::types::{Provenance, SignalCandidate};
use crate::insight::view::OwnedSessionInsightData;

/// Deterministic detector pipeline. Every emitted candidate is promoted to a
/// signal row. Idempotent (INSERT OR REPLACE keyed by derived `signal_id`).
pub async fn run_detectors(pool: &SqlitePool, session_id: &str) -> Result<Vec<SignalRow>> {
    // 매 패스 파일에서 로드 (작은 파일 1회 read/ingest batch) — serve 재시작
    // 없이 튜닝이 반영된다. 결측 → 코드 기본값 (config.rs 계약).
    let cfg = DetectorConfig::load();
    let data = OwnedSessionInsightData::load(pool, session_id).await?;
    let view = data.as_view(session_id);
    let mut rows = Vec::new();
    for det in all_detectors() {
        if !cfg.enabled(det.id()) {
            continue;
        }
        let cands = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            det.detect(&view, &cfg)
        })) {
            Ok(c) => c,
            Err(_) => {
                tracing::warn!(session_id, id = det.id(), "detector panicked; skipping");
                continue;
            }
        };
        // Insert this detector's current-pass signals, then reconcile: delete any
        // stored signal for (session, this detector) NOT in the current pass.
        // This makes the pipeline truly idempotent even for *aggregating* detectors
        // (re_read) whose evidence grows across re-ingests — stale snapshots that
        // were never REPLACE-d (different signal_id) are removed (dogfooding
        // regression 2026-06-11). Each run_detectors call loads the FULL session
        // view, so the current pass is the complete, authoritative set.
        let mut keep_ids = Vec::with_capacity(cands.len());
        for c in cands {
            let row = build_signal_row(session_id, &c, cfg.pack_id());
            keep_ids.push(row.signal_id.clone());
            repo_signal::insert(pool, &row).await?;
            rows.push(row);
        }
        repo_signal::reconcile(pool, session_id, det.id(), &keep_ids).await?;
    }
    Ok(rows)
}

fn build_signal_row(session_id: &str, c: &SignalCandidate, rule_pack: Option<&str>) -> SignalRow {
    let prov = Provenance {
        // Owned String — no `Box::leak`. `run_detectors` runs once per ingest
        // (per OTLP batch for a live session), so leaking a `&'static str` here
        // would grow unbounded over a long-lived `serve`.
        detector: format!("{}@v1", c.detector),
        version: "L1",
        // 03 spec: 코드 기본이면 null, TOML pack 로드 시 그 id. pack이 로드되면
        // 그 패스의 모든 detector에 찍힌다 — 섹션이 없는 detector도 그 pack의
        // 지배(기본값 위임) 아래 돈 것이므로.
        rule_pack: rule_pack.map(String::from),
    };
    // Derive `signal_id` from the stable `dedup_key` when the detector provides
    // one (aggregating detectors like re_read), else from `evidence_refs` (fixed
    // per signal). A stable key keeps one row as evidence grows across re-ingests.
    let id_refs: Vec<String> = match &c.dedup_key {
        Some(k) => vec![k.clone()],
        None => c.evidence_refs.clone(),
    };
    SignalRow {
        signal_id: derive_signal_id(c.detector, session_id, &id_refs),
        schema_version: "signal.v1".into(),
        session_id: session_id.to_string(),
        // `SignalRow.detector` stores the bare id (e.g. `tool_failure`) for
        // simple DB queries/grouping; `Provenance.detector` stores the
        // version-stamped `"{id}@v1"` for audit trails. Intentional, not a bug.
        detector: c.detector.to_string(),
        subkind: c.subkind.map(|s| s.to_string()),
        summary: c.summary.clone(),
        evidence_refs: serde_json::to_string(&c.evidence_refs).unwrap_or_else(|_| "[]".into()),
        facts: c.facts.to_string(),
        provenance: prov.to_json_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
    }
}

/// Returns all detectors for the pipeline, in stable order.
/// `pub(crate)` so the API layer can collect manifests without duplicating the list.
pub(crate) fn all_detectors() -> Vec<Box<dyn crate::insight::extractor::Detector>> {
    use crate::insight::extractors::{
        context_bloat::ContextBloat, re_read::ReRead, risky_action::RiskyAction,
        tool_failure::ToolFailure,
    };
    // final_state_mismatch는 2026-07-03 사용자 결정으로 제거: 영어 고정 lexical
    // 어휘(GOAL_VERBS·COMPLETION_MARKERS)가 "의미 판별은 LLM" 원칙과 긴장했고
    // 비영어 세션에서 발화하지 않았다. 판별은 session-retrospect 스킬(LLM)로
    // 이관 — 측정면(verification_run·SessionMetrics verification_*)은 불변.
    // 기존 signal 행은 migration 0027이 정리(제거된 detector는 reconcile 패스에
    // 안 나타나 zombie가 된다).
    vec![
        Box::new(ToolFailure),
        Box::new(RiskyAction),
        Box::new(ContextBloat),
        Box::new(ReRead),
    ]
}
