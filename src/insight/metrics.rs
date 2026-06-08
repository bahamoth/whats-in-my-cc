//! Deterministic session behavioral metrics (spec §4.2/§5.2). On-demand
//! aggregation over events/signals/verification. Composable counts only —
//! no window-fixed rates (spec F1). No severity/judgment (§6.3). No threshold
//! magic numbers (§6.1).
//!
//! Rate = count / window. Window differs per analysis, so rate is NOT computed
//! here. Consumers derive rate from counts using their own window.
//!
//! No storage table: every call recomputes from the source side-tables.
//! Caching is deferred to a follow-up when call frequency warrants it (§10.1).

use std::collections::BTreeMap;

use sqlx::SqlitePool;

use crate::db::{repo_observed, repo_signal, repo_verification_run};
use crate::error::Result;
use crate::model::observed::EventKind;

/// Session-level deterministic behavioral metrics.
///
/// All fields are composable counts derived from observed facts. No rates,
/// no window-fixed ratios, no severity, no judgment, no threshold-based flags
/// (spec F1 / §6.3 / §6.1).
///
/// `verification_unknown` counts runs whose measurement failed (e.g. process
/// killed before exit-code was captured). It is NOT a failure — consumers must
/// NOT include it in a pass/fail denominator.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionMetrics {
    pub session_id: String,
    /// Total `tool_call` events in the session.
    pub tool_call_total: i64,
    /// Number of `tool_failure` detector signals fired in the session.
    pub tool_failure_count: i64,
    /// Total verification runs recorded for the session.
    pub verification_total: i64,
    /// Verification runs whose `status == "passed"`.
    pub verification_passed: i64,
    /// Verification runs whose `status == "failed"`.
    pub verification_failed: i64,
    /// Verification runs whose `status == "unknown"` — measurement failed,
    /// NOT a failure. Exclude from pass/fail denominators.
    pub verification_unknown: i64,
    /// Number of `context_bloat` detector signals fired in the session.
    pub context_bloat_count: i64,
    /// detector → number of signals fired (signal distribution, spec §6.6).
    pub detector_firing: BTreeMap<String, i64>,
}

/// Compute on-demand behavioral metrics for `session_id`.
///
/// Aggregates in a single pass over each side-table. The three repo calls are
/// independent and could be parallelised in a future optimisation; single
/// sequential reads keep the implementation simple for now.
///
/// # Repo functions used
/// - `repo_observed::list_session(pool, id, 100_000)` — exists and matches.
/// - `repo_signal::list_by_session(pool, id)` — exists (Plan 1).
///   `SignalRow.detector` is the correct field name.
/// - `repo_verification_run::list_session(pool, id)` — exists.
///   `VerificationRunRow.status` is "passed"/"failed"/"unknown".
pub async fn compute_session_metrics(
    pool: &SqlitePool,
    session_id: &str,
) -> Result<SessionMetrics> {
    let events = repo_observed::list_session(pool, session_id, 100_000).await?;
    let signals = repo_signal::list_by_session(pool, session_id).await?;
    let vruns = repo_verification_run::list_session(pool, session_id).await?;

    let tool_call_total = events
        .iter()
        .filter(|e| e.kind == EventKind::ToolCall)
        .count() as i64;

    let mut detector_firing: BTreeMap<String, i64> = BTreeMap::new();
    for s in &signals {
        *detector_firing.entry(s.detector.clone()).or_insert(0) += 1;
    }
    let tool_failure_count = *detector_firing.get("tool_failure").unwrap_or(&0);
    let context_bloat_count = *detector_firing.get("context_bloat").unwrap_or(&0);

    let verification_total = vruns.len() as i64;
    let verification_passed = vruns.iter().filter(|v| v.status == "passed").count() as i64;
    let verification_failed = vruns.iter().filter(|v| v.status == "failed").count() as i64;
    let verification_unknown = vruns.iter().filter(|v| v.status == "unknown").count() as i64;

    Ok(SessionMetrics {
        session_id: session_id.to_string(),
        tool_call_total,
        tool_failure_count,
        verification_total,
        verification_passed,
        verification_failed,
        verification_unknown,
        context_bloat_count,
        detector_firing,
    })
}
