//! Deterministic session behavioral metrics (spec §4.2/§5.2). On-demand
//! aggregation over events/signals/verification/usage. Facts/counts/ratios
//! only — no severity/judgment (§6.3). No threshold magic numbers (§6.1).
//!
//! No storage table: every call recomputes from the source side-tables.
//! Caching is deferred to a follow-up when call frequency warrants it (§10.1).

use std::collections::BTreeMap;

use sqlx::SqlitePool;

use crate::db::{repo_observed, repo_signal, repo_usage_facet, repo_verification_run};
use crate::error::Result;
use crate::model::observed::EventKind;

/// Session-level deterministic behavioral metrics.
///
/// All fields are counts or ratios derived from observed facts. No severity,
/// no judgment, no threshold-based flags (spec §6.3 / §6.1).
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionMetrics {
    pub session_id: String,
    /// Total `tool_call` events in the session.
    pub tool_call_total: i64,
    /// Number of `tool_failure` detector signals fired in the session.
    pub tool_failure_count: i64,
    /// `tool_failure_count / tool_call_total`; 0.0 when denominator is 0.
    pub tool_failure_rate: f64,
    /// Total verification runs recorded for the session.
    pub verification_total: i64,
    /// Verification runs whose `status == "passed"`.
    pub verification_passed: i64,
    /// `verification_passed / verification_total`; 0.0 when denominator is 0.
    pub verification_pass_rate: f64,
    /// Number of `context_bloat` detector signals fired in the session.
    pub context_bloat_count: i64,
    /// `cache_read_input_tokens / (input + cache_read + cache_creation)` from
    /// `usage_facet`; 0.0 when denominator is 0 (no usage rows).
    pub cache_hit_ratio: f64,
    /// detector → number of signals fired (signal distribution, spec §6.6).
    pub detector_firing: BTreeMap<String, i64>,
}

/// `num / den` ratio, returning 0.0 when the denominator is 0.
fn rate(num: i64, den: i64) -> f64 {
    if den == 0 {
        0.0
    } else {
        num as f64 / den as f64
    }
}

/// Compute on-demand behavioral metrics for `session_id`.
///
/// Aggregates in a single pass over each side-table. The four repo calls are
/// independent and could be parallelised in a future optimisation; single
/// sequential reads keep the implementation simple for now.
///
/// # Repo functions used (vs. plan guesses)
/// - `repo_observed::list_session(pool, id, 100_000)` — plan guessed this
///   name, it exists and matches.
/// - `repo_signal::list_by_session(pool, id)` — plan guessed this name, it
///   exists (Plan 1). `SignalRow.detector` is the correct field name.
/// - `repo_verification_run::list_session(pool, id)` — plan guessed this
///   name, it exists. `VerificationRunRow.status == "passed"` is correct.
/// - `repo_usage_facet::session_aggregate(pool, id)` — plan guessed
///   `repo_usage_facet::list_session` (which does NOT exist). The real
///   existing aggregate fn is `session_aggregate`, returning `UsageAggregate`
///   with `input_tokens`, `cache_read_input_tokens`,
///   `cache_creation_input_tokens`. Reused as-is.
pub async fn compute_session_metrics(pool: &SqlitePool, session_id: &str) -> Result<SessionMetrics> {
    // Load source data. Large sessions use a high limit for the event scan;
    // see DEV note if this becomes a bottleneck (candidate for SQL COUNT).
    let events = repo_observed::list_session(pool, session_id, 100_000).await?;
    let signals = repo_signal::list_by_session(pool, session_id).await?;
    let vruns = repo_verification_run::list_session(pool, session_id).await?;
    let usage = repo_usage_facet::session_aggregate(pool, session_id).await?;

    // --- tool_call_total ---------------------------------------------------
    let tool_call_total = events
        .iter()
        .filter(|e| e.kind == EventKind::ToolCall)
        .count() as i64;

    // --- detector_firing + derived scalar counts --------------------------
    let mut detector_firing: BTreeMap<String, i64> = BTreeMap::new();
    for s in &signals {
        *detector_firing.entry(s.detector.clone()).or_insert(0) += 1;
    }
    let tool_failure_count = *detector_firing.get("tool_failure").unwrap_or(&0);
    let context_bloat_count = *detector_firing.get("context_bloat").unwrap_or(&0);

    // --- verification runs ------------------------------------------------
    let verification_total = vruns.len() as i64;
    let verification_passed = vruns.iter().filter(|v| v.status == "passed").count() as i64;

    // --- cache hit ratio from usage aggregate -----------------------------
    let cache_read = usage.cache_read_input_tokens;
    let total_in = usage.input_tokens + usage.cache_read_input_tokens + usage.cache_creation_input_tokens;

    Ok(SessionMetrics {
        session_id: session_id.to_string(),
        tool_call_total,
        tool_failure_count,
        tool_failure_rate: rate(tool_failure_count, tool_call_total),
        verification_total,
        verification_passed,
        verification_pass_rate: rate(verification_passed, verification_total),
        context_bloat_count,
        cache_hit_ratio: rate(cache_read, total_in),
        detector_firing,
    })
}
