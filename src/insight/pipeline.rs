//! Deterministic L1-only extractor pipeline (Phase A refactor).
//!
//! All candidates whose `confidence_l1 >= floor.max(CONFIDENCE_FLOOR)` are
//! promoted directly to active findings. No judge, no pending queue, no L2.
//!
//! `run_extractors` is idempotent: re-running produces the same `finding_id`s;
//! `INSERT OR REPLACE` keeps the last writer's version (same content).
//!
//! Called directly from each ingest path (transcript / OTel traces·logs·metrics
//! / hook) once observed events for a session have been committed.

use sqlx::SqlitePool;

use crate::db::repo_finding::{self, FindingRow};
use crate::error::Result;
use crate::ids::derive_finding_id;
use crate::insight::types::{FindingCandidate, Provenance};
use crate::insight::view::OwnedSessionInsightData;

/// Minimum confidence below which a candidate is dropped.
pub const CONFIDENCE_FLOOR: f32 = 0.5;

/// Deterministic L1 finding pipeline. Every candidate >= its floor is promoted
/// directly to an active finding. Idempotent (INSERT OR REPLACE).
pub async fn run_extractors(pool: &SqlitePool, session_id: &str) -> Result<Vec<FindingRow>> {
    let data = OwnedSessionInsightData::load(pool, session_id).await?;
    let view = data.as_view(session_id);
    let mut rows = Vec::new();
    for ext in all_extractors_for_pipeline() {
        let category = ext.category();
        let floor = ext.floor();
        let cands = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ext.extract(&view)
        })) {
            Ok(c) => c,
            Err(_) => {
                tracing::warn!(session_id, category, "extractor panicked; skipping");
                continue;
            }
        };
        for c in cands {
            if c.confidence_l1 < floor.max(CONFIDENCE_FLOOR) {
                tracing::debug!(category, confidence = c.confidence_l1, "dropped below floor");
                continue;
            }
            let row = build_l1_row(session_id, &c);
            repo_finding::insert(pool, &row).await?;
            rows.push(row);
        }
    }
    Ok(rows)
}

fn build_l1_row(session_id: &str, c: &FindingCandidate) -> FindingRow {
    let extractor_id = format!("{}@v1", c.category);
    let prov = Provenance {
        extractor: Box::leak(extractor_id.into_boxed_str()),
        layer: "L1",
        judge: None,
        judge_template_version: None,
        rule_pack: None,
    };
    let finding_id = derive_finding_id(c.category, session_id, &c.evidence_refs);
    FindingRow {
        finding_id,
        schema_version: "finding.v1".into(),
        session_id: session_id.to_string(),
        category: c.category.to_string(),
        subkind: c.subkind.map(|s| s.to_string()),
        severity: c.severity.to_string(),
        confidence: c.confidence_l1 as f64,
        summary: c.summary.clone(),
        evidence_refs: serde_json::to_string(&c.evidence_refs).unwrap_or_else(|_| "[]".into()),
        evidence_projection: c.evidence_projection.to_string(),
        provenance: prov.to_json_string(),
        status: "active".into(),
        created_at: chrono::Utc::now().to_rfc3339(),
    }
}

/// Returns all extractors for the pipeline.
/// Production builds only include the production extractors.
fn all_extractors_for_pipeline() -> Vec<Box<dyn crate::insight::extractor::InsightExtractor>> {
    use crate::insight::extractors::{
        context_bloat::ContextBloat,
        final_state_mismatch::FinalStateMismatch,
        risky_action::RiskyAction,
        tool_failure::ToolFailure,
    };
    vec![
        Box::new(ToolFailure),
        Box::new(RiskyAction),
        Box::new(ContextBloat),
        Box::new(FinalStateMismatch),
    ]
}
