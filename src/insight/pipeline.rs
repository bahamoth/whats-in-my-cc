//! Extractor pipeline runner (slice-14 + slice-15).
//!
//! `run_extractors` — original L1-only entry point (kept for back-compat).
//! `run_extractors_with_runtime` — slice-15 L2-aware entry point.
//!
//! Promotion logic per `PromotionPolicy`:
//!   - `Always`        → insert finding directly (L1, no judge)
//!   - `Never`         → always queue for judge; if judge disabled/budget, → pending
//!   - `IfAbove(t)`    → promote if `confidence_l1 > t`; else queue for judge
//!
//! When the judge returns `Err(Disabled)` or `Err(BudgetExhausted)` the candidate
//! goes to `findings_pending_judge` for the next rebuild pass.

use sqlx::SqlitePool;

use crate::db::repo_finding::{self, FindingRow};
use crate::db::repo_findings_pending::{self, PendingFindingRow};
use crate::error::Result;
use crate::ids::derive_finding_id;
use crate::insight::judge::runtime::JudgeRuntime;
use crate::insight::judge::{JudgeError, JudgePrompt, JudgeProvider};
use crate::insight::types::{FindingCandidate, PromotionPolicy, Provenance};
use crate::insight::view::OwnedSessionInsightData;

/// Minimum confidence below which a candidate is dropped.
pub const CONFIDENCE_FLOOR: f32 = 0.5;

/// Original L1-only entry point. Calls `run_extractors_with_runtime` with a noop
/// runtime so existing callers (graph rebuild, ingest) keep working unchanged.
///
/// Idempotent: re-running produces the same `finding_id`s; `INSERT OR REPLACE`
/// keeps the last writer's version (same content).
///
/// Per DEV-S14-06: runs **after** the graph rebuild transaction commits, not
/// inside it.
pub async fn run_extractors(pool: &SqlitePool, session_id: &str) -> Result<Vec<FindingRow>> {
    let runtime = JudgeRuntime::noop();
    run_extractors_with_runtime(pool, session_id, &runtime).await
}

/// Slice-15 L2-aware pipeline. Accepts a `JudgeRuntime` reference; builds a
/// fresh per-rebuild `BudgetGuardDyn` provider for this invocation.
///
/// Algorithm:
/// 1. Load session view.
/// 2. Drain `findings_pending_judge` for this session (prior budget-exhausted
///    candidates); attempt to judge them with the current runtime.
/// 3. Run all registered L1 extractors; apply confidence floor; route each
///    candidate through `PromotionPolicy`.
/// 4. Return the list of `FindingRow`s written this pass.
pub async fn run_extractors_with_runtime(
    pool: &SqlitePool,
    session_id: &str,
    runtime: &JudgeRuntime,
) -> Result<Vec<FindingRow>> {
    let data = OwnedSessionInsightData::load(pool, session_id).await?;
    let view = data.as_view(session_id);
    let judge = runtime.build_for_rebuild();

    let mut rows: Vec<FindingRow> = Vec::new();

    // --- Step 1: drain the pending queue from prior passes ---
    let pending = repo_findings_pending::list_session(pool, session_id).await?;
    for p in pending {
        let evidence_projection: serde_json::Value =
            serde_json::from_str(&p.evidence_projection).unwrap_or(serde_json::Value::Null);
        let prompt = JudgePrompt {
            category: p.category.clone(),
            candidate_id: p.candidate_id.clone(),
            evidence_projection: evidence_projection.clone(),
            system_template: String::new(), // runtime selects template
        };
        match judge.judge(prompt).await {
            Ok(verdict) => {
                repo_findings_pending::dequeue(pool, &p.candidate_id).await?;
                if verdict.promote {
                    let prov = Provenance {
                        extractor: Box::leak(format!("{}@v1", p.category).into_boxed_str()),
                        layer: "L2",
                        judge: Some(judge.model_id().to_string()),
                        judge_template_version: Some(
                            judge.prompt_template_version().to_string(),
                        ),
                        rule_pack: None,
                    };
                    let row = FindingRow {
                        finding_id: p.candidate_id.clone(),
                        schema_version: "finding.v1".into(),
                        session_id: session_id.to_string(),
                        category: p.category.clone(),
                        severity: "low".into(),
                        confidence: verdict.confidence_l2 as f64,
                        summary: verdict.reason.clone(),
                        evidence_refs: p.evidence_refs.clone(),
                        evidence_projection: p.evidence_projection.clone(),
                        provenance: prov.to_json_string(),
                        status: "active".into(),
                        created_at: chrono::Utc::now().to_rfc3339(),
                    };
                    repo_finding::insert(pool, &row).await?;
                    rows.push(row);
                }
            }
            Err(JudgeError::Disabled) | Err(JudgeError::BudgetExhausted) => {
                // leave in pending for next pass
                repo_findings_pending::record_attempt(pool, &p.candidate_id).await?;
            }
            Err(e) => {
                tracing::warn!(
                    session_id,
                    candidate_id = %p.candidate_id,
                    error = ?e,
                    "judge error on pending candidate; leaving in queue"
                );
                repo_findings_pending::record_attempt(pool, &p.candidate_id).await?;
            }
        }
    }

    // --- Step 2: run L1 extractors ---
    let extractors = all_extractors_for_pipeline();
    for ext in &extractors {
        let category = ext.category();
        let policy = ext.promotion_policy();
        let floor = ext.floor();

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
            if c.confidence_l1 < floor.max(CONFIDENCE_FLOOR) {
                tracing::debug!(category, confidence = c.confidence_l1, "dropped below floor");
                continue;
            }
            route_candidate(pool, session_id, c, policy, &*judge, &mut rows).await?;
        }
    }

    Ok(rows)
}

/// Route a single candidate according to its `PromotionPolicy`.
async fn route_candidate(
    pool: &SqlitePool,
    session_id: &str,
    c: FindingCandidate,
    policy: PromotionPolicy,
    judge: &dyn JudgeProvider,
    rows: &mut Vec<FindingRow>,
) -> Result<()> {
    match policy {
        PromotionPolicy::Always => {
            // L1 direct promote — no judge involvement.
            let row = build_l1_row(session_id, &c);
            repo_finding::insert(pool, &row).await?;
            rows.push(row);
        }
        PromotionPolicy::Never => {
            // Always routes through L2 judge.
            judge_or_queue(pool, session_id, c, judge, rows).await?;
        }
        PromotionPolicy::IfAbove(threshold) => {
            if c.confidence_l1 > threshold {
                let row = build_l1_row(session_id, &c);
                repo_finding::insert(pool, &row).await?;
                rows.push(row);
            } else {
                judge_or_queue(pool, session_id, c, judge, rows).await?;
            }
        }
    }
    Ok(())
}

/// Attempt to judge a candidate. On success + promote: write finding.
/// On Disabled / BudgetExhausted: enqueue to pending. On other error: log + queue.
async fn judge_or_queue(
    pool: &SqlitePool,
    session_id: &str,
    c: FindingCandidate,
    judge: &dyn JudgeProvider,
    rows: &mut Vec<FindingRow>,
) -> Result<()> {
    let candidate_id = derive_finding_id(c.category, session_id, &c.evidence_refs);
    let prompt = JudgePrompt {
        category: c.category.to_string(),
        candidate_id: candidate_id.clone(),
        evidence_projection: c.evidence_projection.clone(),
        system_template: String::new(),
    };
    match judge.judge(prompt).await {
        Ok(verdict) => {
            if verdict.promote {
                let prov = Provenance {
                    extractor: Box::leak(format!("{}@v1", c.category).into_boxed_str()),
                    layer: "L2",
                    judge: Some(judge.model_id().to_string()),
                    judge_template_version: Some(judge.prompt_template_version().to_string()),
                    rule_pack: None,
                };
                let row = FindingRow {
                    finding_id: candidate_id,
                    schema_version: "finding.v1".into(),
                    session_id: session_id.to_string(),
                    category: c.category.to_string(),
                    severity: c.severity.to_string(),
                    confidence: verdict.confidence_l2 as f64,
                    summary: verdict.reason.clone(),
                    evidence_refs: serde_json::to_string(&c.evidence_refs)
                        .unwrap_or_else(|_| "[]".into()),
                    evidence_projection: c.evidence_projection.to_string(),
                    provenance: prov.to_json_string(),
                    status: "active".into(),
                    created_at: chrono::Utc::now().to_rfc3339(),
                };
                repo_finding::insert(pool, &row).await?;
                rows.push(row);
            }
        }
        Err(JudgeError::Disabled) | Err(JudgeError::BudgetExhausted) => {
            enqueue_pending(pool, session_id, candidate_id, &c).await?;
        }
        Err(e) => {
            tracing::warn!(
                session_id,
                category = c.category,
                error = ?e,
                "judge error; queuing to pending"
            );
            enqueue_pending(pool, session_id, candidate_id, &c).await?;
        }
    }
    Ok(())
}

/// Enqueue a candidate to `findings_pending_judge`.
async fn enqueue_pending(
    pool: &SqlitePool,
    session_id: &str,
    candidate_id: String,
    c: &FindingCandidate,
) -> Result<()> {
    let row = PendingFindingRow {
        candidate_id,
        schema_version: "pending_finding.v1".into(),
        session_id: session_id.to_string(),
        category: c.category.to_string(),
        confidence_l1: c.confidence_l1,
        evidence_refs: serde_json::to_string(&c.evidence_refs).unwrap_or_else(|_| "[]".into()),
        evidence_projection: c.evidence_projection.to_string(),
    };
    // INSERT OR IGNORE — idempotent; if already pending just leave it.
    repo_findings_pending::enqueue(pool, &row).await?;
    Ok(())
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

/// Returns all extractors for the pipeline, including the cfg(test) NoopTestExtractor.
/// Production builds only include the production extractors.
fn all_extractors_for_pipeline() -> Vec<Box<dyn crate::insight::extractor::InsightExtractor>> {
    use crate::insight::extractors::{
        context_bloat::ContextBloat,
        final_state_mismatch::FinalStateMismatch,
        missing_verification::MissingVerification,
        risky_action::RiskyAction,
        tool_failure::ToolFailure,
    };
    #[allow(unused_mut)]
    let mut v: Vec<Box<dyn crate::insight::extractor::InsightExtractor>> = vec![
        Box::new(MissingVerification),
        Box::new(ToolFailure),
        Box::new(RiskyAction),
        Box::new(ContextBloat),
        Box::new(FinalStateMismatch),
    ];
    #[cfg(feature = "test-helpers")]
    {
        use crate::insight::extractors::noop_test::NoopTestExtractor;
        v.push(Box::new(NoopTestExtractor));
    }
    v
}
