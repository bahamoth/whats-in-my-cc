//! Slice-19 — Retention sweep.
//!
//! Stub — will be filled in commit 6 after migrations are in place.

use std::collections::HashMap;

use anyhow::Result;
use sqlx::SqlitePool;

/// Retention profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Profile {
    /// No deletion (default). Capability ships off by default.
    None,
    /// Default profile: raw 30d, normalized 180d, graph/insight 180d, audit 90d, judge_cache 30d.
    Default,
    /// Strict profile: raw 7d, normalized 30d, graph/insight 30d, audit 30d, judge_cache 7d.
    Strict,
}

impl Profile {
    /// Raw payload retention in days. None = keep forever.
    pub fn raw_payload_days(&self) -> Option<i64> {
        match self {
            Profile::None => None,
            Profile::Default => Some(30),
            Profile::Strict => Some(7),
        }
    }

    /// Normalized events retention in days.
    pub fn normalized_event_days(&self) -> Option<i64> {
        match self {
            Profile::None => None,
            Profile::Default => Some(180),
            Profile::Strict => Some(30),
        }
    }

    /// Graph & insight retention in days.
    pub fn graph_insight_days(&self) -> Option<i64> {
        match self {
            Profile::None => None,
            Profile::Default => Some(180),
            Profile::Strict => Some(30),
        }
    }

    /// Audit table retention in days.
    pub fn audit_days(&self) -> Option<i64> {
        match self {
            Profile::None => None,
            Profile::Default => Some(90),
            Profile::Strict => Some(30),
        }
    }

    /// Judge cache retention in days (by last_hit_at).
    pub fn judge_cache_days(&self) -> Option<i64> {
        match self {
            Profile::None => None,
            Profile::Default => Some(30),
            Profile::Strict => Some(7),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Profile::None => "none",
            Profile::Default => "default",
            Profile::Strict => "strict",
        }
    }
}

impl std::str::FromStr for Profile {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        match s {
            "none" => Ok(Profile::None),
            "default" => Ok(Profile::Default),
            "strict" => Ok(Profile::Strict),
            _ => Err(anyhow::anyhow!("unknown retention profile: {s}; expected none|default|strict")),
        }
    }
}

/// Retention policy (wraps the profile).
#[derive(Debug, Clone)]
pub struct RetentionPolicy {
    pub profile: Profile,
}

/// Result of a single sweep pass.
#[derive(Debug, Default)]
pub struct SweepReport {
    /// Counts of deleted rows per class.
    pub deletions: HashMap<String, u64>,
}

/// Run a single sweep pass: delete rows older than the configured retention
/// thresholds, write tombstones, and append an audit row.
pub async fn run_sweep(pool: &SqlitePool, policy: &RetentionPolicy) -> Result<SweepReport> {
    let mut report = SweepReport::default();

    if policy.profile == Profile::None {
        // No-op: write no audit row, delete nothing.
        return Ok(report);
    }

    // ---- raw_event --------------------------------------------------------
    if let Some(days) = policy.profile.raw_payload_days() {
        let cutoff = format!("-{days} days");
        // Collect ids to tombstone before deleting
        let ids: Vec<(String,)> = sqlx::query_as(
            "SELECT raw_event_id FROM raw_event WHERE captured_at < datetime('now', ?)",
        )
        .bind(&cutoff)
        .fetch_all(pool)
        .await?;

        let count = ids.len() as u64;
        for (id,) in &ids {
            sqlx::query(
                "INSERT OR IGNORE INTO retention_tombstone (resource_id, resource_kind) VALUES (?, 'raw_event')",
            )
            .bind(id)
            .execute(pool)
            .await?;
        }
        if count > 0 {
            sqlx::query("DELETE FROM raw_event WHERE captured_at < datetime('now', ?)")
                .bind(&cutoff)
                .execute(pool)
                .await?;
        }
        report.deletions.insert("raw_event".to_string(), count);
    }

    // ---- Write audit row --------------------------------------------------
    let audit_id = format!("aud_{}", ulid::Ulid::new());
    let payload = serde_json::to_string(&report.deletions).unwrap_or_else(|_| "{}".into());
    sqlx::query(
        "INSERT INTO audit (audit_id, event, actor, payload) VALUES (?, 'retention.deleted', 'retention_sweep', ?)",
    )
    .bind(&audit_id)
    .bind(&payload)
    .execute(pool)
    .await?;

    Ok(report)
}

/// Spawn a background sweep task that wakes every 6 hours.
/// No-op when `policy.profile == Profile::None`.
pub fn spawn_sweep_task(
    pool: SqlitePool,
    policy: RetentionPolicy,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        if policy.profile == Profile::None {
            return;
        }
        let mut interval =
            tokio::time::interval(std::time::Duration::from_secs(6 * 60 * 60));
        loop {
            interval.tick().await;
            if let Err(e) = run_sweep(&pool, &policy).await {
                tracing::warn!(error = ?e, "retention sweep failed");
            }
        }
    })
}

/// State that the health endpoint tracks about the last sweep.
#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct SweepStats {
    pub last_sweep_at: Option<String>,
    pub last_sweep_deletions: HashMap<String, u64>,
}
