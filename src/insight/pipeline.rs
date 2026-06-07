//! Deterministic signal detector pipeline (Plan 1: finding → signal).
//!
//! Every `SignalCandidate` a detector emits is promoted directly to a `signal`
//! row. No judge, no confidence floor, no pending queue — detectors emit facts,
//! not judgments (spec §6.3).
//!
//! `run_detectors` is idempotent: re-running produces the same `signal_id`s;
//! `INSERT OR REPLACE` keeps the last writer's version (same content).
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
    // Plan 4에서 파일 로드로 교체; 지금은 코드 default.
    let cfg = DetectorConfig::default();
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
        for c in cands {
            let row = build_signal_row(session_id, &c);
            repo_signal::insert(pool, &row).await?;
            rows.push(row);
        }
    }
    Ok(rows)
}

fn build_signal_row(session_id: &str, c: &SignalCandidate) -> SignalRow {
    let prov = Provenance {
        // Owned String — no `Box::leak`. `run_detectors` runs once per ingest
        // (per OTLP batch for a live session), so leaking a `&'static str` here
        // would grow unbounded over a long-lived `serve`.
        detector: format!("{}@v1", c.detector),
        version: "L1",
        rule_pack: None,
    };
    SignalRow {
        signal_id: derive_signal_id(c.detector, session_id, &c.evidence_refs),
        schema_version: "signal.v1".into(),
        session_id: session_id.to_string(),
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
fn all_detectors() -> Vec<Box<dyn crate::insight::extractor::Detector>> {
    use crate::insight::extractors::{
        context_bloat::ContextBloat, final_state_mismatch::FinalStateMismatch,
        risky_action::RiskyAction, tool_failure::ToolFailure,
    };
    vec![
        Box::new(ToolFailure),
        Box::new(RiskyAction),
        Box::new(ContextBloat),
        Box::new(FinalStateMismatch),
    ]
}
