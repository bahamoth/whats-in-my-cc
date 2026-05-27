//! Extractor pipeline runner (slice-14).
//!
//! `run_extractors` loads the session view once, runs all registered L1
//! extractors, applies the confidence floor (0.5), derives finding_id, and
//! writes finding rows via `INSERT OR REPLACE` (idempotent).

use sqlx::SqlitePool;

use crate::db::repo_finding::{self, FindingRow};
use crate::error::Result;
use crate::ids::derive_finding_id;
use crate::insight::registry::all_extractors;
use crate::insight::types::Provenance;
use crate::insight::view::OwnedSessionInsightData;
use crate::model::meta::SCHEMA_VERSION;

/// Minimum confidence below which a candidate is dropped.
pub const CONFIDENCE_FLOOR: f32 = 0.5;

/// Run all registered extractors for `session_id`, persist findings, and
/// return the list of rows written.
///
/// Idempotent: re-running produces the same `finding_id`s; `INSERT OR REPLACE`
/// keeps the last writer's version (same content).
///
/// Per DEV-S14-06: runs **after** the graph rebuild transaction commits, not
/// inside it.
pub async fn run_extractors(pool: &SqlitePool, session_id: &str) -> Result<Vec<FindingRow>> {
    let data = OwnedSessionInsightData::load(pool, session_id).await?;
    let view = data.as_view(session_id);

    let extractors = all_extractors();
    let mut rows: Vec<FindingRow> = Vec::new();

    for ext in &extractors {
        // Catch extractor panics per spec §9 / DEV-S14-06.
        let category = ext.category();
        let cands_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ext.extract(&view)
        }));
        let cands = match cands_result {
            Ok(c) => c,
            Err(_) => {
                tracing::warn!(
                    session_id,
                    category,
                    "extractor panicked; skipping category for this session"
                );
                continue;
            }
        };

        for c in cands {
            if c.confidence_l1 < CONFIDENCE_FLOOR {
                tracing::debug!(category, confidence = c.confidence_l1, "dropped below floor");
                continue;
            }

            // Build provenance
            let extractor_id = format!("{category}@v1");
            let prov = Provenance {
                extractor: Box::leak(extractor_id.into_boxed_str()),
                layer: "L1",
                judge: None,
                judge_template_version: None,
                rule_pack: None,
            };

            // Derive deterministic finding_id
            let finding_id = derive_finding_id(category, session_id, &c.evidence_refs);

            let row = FindingRow {
                finding_id,
                schema_version: "finding.v1".into(),
                session_id: session_id.to_string(),
                category: category.to_string(),
                severity: c.severity.to_string(),
                confidence: c.confidence_l1 as f64,
                summary: c.summary.clone(),
                evidence_refs: serde_json::to_string(&c.evidence_refs).unwrap_or_else(|_| "[]".into()),
                evidence_projection: c.evidence_projection.to_string(),
                provenance: prov.to_json_string(),
                status: "active".into(),
                created_at: chrono::Utc::now().to_rfc3339(),
            };

            repo_finding::insert(pool, &row).await?;
            rows.push(row);
        }
    }

    Ok(rows)
}
