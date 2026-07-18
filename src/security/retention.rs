//! Slice-19 + full-retention (2026-06-11) — Retention sweep.
//!
//! Enforces every data class of the active profile (docs/05 §05):
//!
//! - **raw payload** — *scrubbed, not row-deleted*: `payload` is emptied while
//!   the skeleton row (source_uri / line_no / sha) survives. The skeleton keeps
//!   the `observed_event.raw_event_id` FK valid (raw expires *before* its
//!   normalized children) and keeps the ingest dedup triple, so an
//!   `ingest --all` replay cannot resurrect expired content. Tombstone per
//!   scrubbed payload → `/v1/events/:id/raw` answers 410.
//! - **normalized + insight** — session granularity ("delete by session",
//!   docs/05): a session whose newest *activity* timestamp (`observed_at` of
//!   observed_event/usage_facet, `started_at` of verification_run) is older
//!   than the normalized cutoff loses its observed_event / diff_hunk /
//!   verification_run / usage_facet / signal rows — including sessions that
//!   exist only in side tables. Ingest-time `created_at` columns are not
//!   consulted (re-ingest must not extend retention).
//!   Tombstones: the session id, every signal id,
//!   every verification_run id. (Per-event tombstones are deliberately not
//!   written — a session can hold 10k+ events and tombstones live forever,
//!   DEV-S19-04; expired events answer 404, the session id answers 410.)
//!   Insight shares the session pass — `insight_window_matches_normalized_window`
//!   in `tests/retention_sweep.rs` locks the window equality this relies on.
//! - **audit** — rows older than the audit cutoff are deleted; the sweep's own
//!   `retention.deleted` row is written afterwards inside the same transaction.
//!
//! The whole sweep is ONE sqlx transaction with all cutoffs derived from a
//! single `Utc::now()`, so cancellation mid-sweep rolls back atomically and
//! the SELECT/DELETE phases can never disagree about "now".

use std::collections::HashMap;

use anyhow::Result;
use sqlx::SqlitePool;

/// Retention profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Profile {
    /// No deletion — explicit archival opt-out (`--retention-profile none`).
    None,
    /// Default profile (CLI default since 2026-07-08): raw 60d,
    /// normalized 180d, insight 180d, audit 90d.
    Default,
    /// Strict profile: raw 7d, normalized 30d, insight 30d, audit 30d.
    Strict,
}

impl Profile {
    /// Raw payload retention in days. None = keep forever.
    pub fn raw_payload_days(&self) -> Option<i64> {
        match self {
            Profile::None => None,
            Profile::Default => Some(60),
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

    /// Insight (signal) retention in days. Swept together with the normalized
    /// class at session granularity; must stay equal to
    /// `normalized_event_days` (locked by test) or the sweep needs its own pass.
    pub fn insight_days(&self) -> Option<i64> {
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
            _ => Err(anyhow::anyhow!(
                "unknown retention profile: {s}; expected none|default|strict"
            )),
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
    /// Counts per class: `raw_event` (payloads scrubbed), `session`,
    /// `observed_event`, `diff_hunk`, `verification_run`, `usage_facet`,
    /// `signal`, `session_summary`, `instruction_observation`,
    /// `instruction_snapshot` (orphans), `ingest_run` (unreferenced),
    /// `audit` (rows deleted).
    pub deletions: HashMap<String, u64>,
}

/// Stored timestamps come from two writers: Rust (`to_rfc3339`,
/// `2026-06-11T12:34:56.789+00:00`) and SQLite column defaults
/// (`datetime('now')`, `2026-06-11 12:34:56`). Lexical `<` across the two
/// formats is exact across calendar days (' ' < 'T') and off by at most the
/// cutoff day itself — immaterial at day-granularity retention windows.
fn cutoff_rfc3339(now: chrono::DateTime<chrono::Utc>, days: i64) -> String {
    (now - chrono::Duration::days(days)).to_rfc3339()
}

/// Cutoff in SQLite `datetime('now')` format, for columns only ever written by
/// column defaults (`audit.created_at`).
fn cutoff_sqlite(now: chrono::DateTime<chrono::Utc>, days: i64) -> String {
    (now - chrono::Duration::days(days))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

async fn insert_tombstone_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    resource_id: &str,
    resource_kind: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT OR IGNORE INTO retention_tombstone (resource_id, resource_kind) VALUES (?, ?)",
    )
    .bind(resource_id)
    .bind(resource_kind)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Run a single sweep pass: scrub expired raw payloads, delete expired
/// sessions' normalized + insight rows, prune the audit table, write
/// tombstones, and append one `retention.deleted` audit row — all in one
/// transaction.
pub async fn run_sweep(pool: &SqlitePool, policy: &RetentionPolicy) -> Result<SweepReport> {
    let mut report = SweepReport::default();

    if policy.profile == Profile::None {
        // No-op: write no audit row, delete nothing.
        return Ok(report);
    }

    let now = chrono::Utc::now();
    let mut tx = pool.begin().await?;

    // ---- raw payload: scrub, keep skeleton -------------------------------
    if let Some(days) = policy.profile.raw_payload_days() {
        let cutoff = cutoff_rfc3339(now, days);
        // `length(payload) > 0` makes the scrub idempotent: already-scrubbed
        // rows (and rows that never had content) are not re-counted.
        let ids: Vec<(String,)> = sqlx::query_as(
            "SELECT raw_event_id FROM raw_event \
             WHERE captured_at < ? AND length(payload) > 0",
        )
        .bind(&cutoff)
        .fetch_all(&mut *tx)
        .await?;

        for (id,) in &ids {
            insert_tombstone_tx(&mut tx, id, "raw_event").await?;
            sqlx::query("UPDATE raw_event SET payload = x'' WHERE raw_event_id = ?")
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
        report
            .deletions
            .insert("raw_event".to_string(), ids.len() as u64);
    }

    // ---- normalized + insight: session granularity -----------------------
    if let Some(days) = policy.profile.normalized_event_days() {
        let cutoff = cutoff_rfc3339(now, days);
        // Candidates come from every table carrying a *session-activity*
        // timestamp, so side-table rows whose session has no observed_event
        // (partially ingested data) still expire. `signal.created_at` /
        // `diff_hunk.created_at` are deliberately excluded: they are ingest
        // time, and re-ingest (routine here) would reset retention forever.
        let sessions: Vec<(String,)> = sqlx::query_as(
            "SELECT session_id FROM (\
                 SELECT session_id, observed_at AS ts FROM observed_event \
                 UNION ALL SELECT session_id, started_at FROM verification_run \
                 UNION ALL SELECT session_id, observed_at FROM usage_facet \
                 UNION ALL SELECT session_id, observed_at FROM instruction_observation\
             ) GROUP BY session_id HAVING MAX(ts) < ?",
        )
        .bind(&cutoff)
        .fetch_all(&mut *tx)
        .await?;

        let mut counts: HashMap<&'static str, u64> = HashMap::new();
        for (sid,) in &sessions {
            let signal_ids: Vec<(String,)> =
                sqlx::query_as("SELECT signal_id FROM signal WHERE session_id = ?")
                    .bind(sid)
                    .fetch_all(&mut *tx)
                    .await?;
            for (id,) in &signal_ids {
                insert_tombstone_tx(&mut tx, id, "signal").await?;
            }
            let run_ids: Vec<(String,)> = sqlx::query_as(
                "SELECT verification_run_id FROM verification_run WHERE session_id = ?",
            )
            .bind(sid)
            .fetch_all(&mut *tx)
            .await?;
            for (id,) in &run_ids {
                insert_tombstone_tx(&mut tx, id, "verification_run").await?;
            }

            // growth-2026-07-18: session_summary (0025) and
            // instruction_observation (0028) postdate the original sweep and
            // were grow-only — they expire with their session like the rest
            // of the session-granularity class. No per-row tombstones: the
            // session tombstone already answers 410 for the session, and
            // neither table is an individually addressable resource class.
            for table in [
                "signal",
                "verification_run",
                "diff_hunk",
                "usage_facet",
                "observed_event",
                "session_summary",
                "instruction_observation",
            ] {
                let res = sqlx::query(&format!("DELETE FROM {table} WHERE session_id = ?"))
                    .bind(sid)
                    .execute(&mut *tx)
                    .await?;
                *counts.entry(table).or_default() += res.rows_affected();
            }
            insert_tombstone_tx(&mut tx, sid, "session").await?;
        }
        report
            .deletions
            .insert("session".to_string(), sessions.len() as u64);
        for (table, n) in counts {
            report.deletions.insert(table.to_string(), n);
        }

        // Content-addressed snapshots are shared across sessions; one loses
        // its row only when no surviving observation references it.
        let res = sqlx::query(
            "DELETE FROM instruction_snapshot WHERE content_sha256 NOT IN \
             (SELECT content_sha256 FROM instruction_observation)",
        )
        .execute(&mut *tx)
        .await?;
        report
            .deletions
            .insert("instruction_snapshot".to_string(), res.rows_affected());
    }

    // ---- audit ------------------------------------------------------------
    if let Some(days) = policy.profile.audit_days() {
        let cutoff = cutoff_sqlite(now, days);
        let res = sqlx::query("DELETE FROM audit WHERE created_at < ?")
            .bind(&cutoff)
            .execute(&mut *tx)
            .await?;
        report
            .deletions
            .insert("audit".to_string(), res.rows_affected());

        // growth-2026-07-18: ingest_run is operational metadata (same class
        // as audit). Rows referenced by a raw skeleton must stay (FK +
        // provenance); unreferenced ones — historical accumulation from
        // before the live path cleaned up its own empty runs — age out with
        // the audit window.
        let cutoff = cutoff_rfc3339(now, days);
        let res = sqlx::query(
            "DELETE FROM ingest_run WHERE started_at < ? \
             AND run_id NOT IN (SELECT ingest_run_id FROM raw_event)",
        )
        .bind(&cutoff)
        .execute(&mut *tx)
        .await?;
        report
            .deletions
            .insert("ingest_run".to_string(), res.rows_affected());
    }

    // ---- Write audit row --------------------------------------------------
    let audit_id = format!("aud_{}", ulid::Ulid::new());
    let payload = serde_json::to_string(&report.deletions).unwrap_or_else(|_| "{}".into());
    sqlx::query(
        "INSERT INTO audit (audit_id, event, actor, payload) VALUES (?, 'retention.deleted', 'retention_sweep', ?)",
    )
    .bind(&audit_id)
    .bind(&payload)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    // ---- disk reclaim -----------------------------------------------------
    // growth-2026-07-18: without this the main .sqlite file never shrinks —
    // deletes only move pages to the freelist. `incremental_vacuum` releases
    // them (no-op on DBs created before auto_vacuum=INCREMENTAL; a manual
    // VACUUM converts those), and the TRUNCATE checkpoint folds the WAL back
    // so the shrink is visible on disk. Outside the transaction by necessity.
    sqlx::query("PRAGMA incremental_vacuum")
        .execute(pool)
        .await?;
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(pool)
        .await?;

    Ok(report)
}

/// Spawn a background sweep task that wakes every 6 hours.
/// No-op when `policy.profile == Profile::None`.
///
/// `cancel` is observed at every loop iteration AND during `run_sweep`:
/// if cancellation fires mid-sweep the in-progress sqlx Transaction is
/// dropped (auto rollback), no partial deletion is committed.
pub fn spawn_sweep_task(
    pool: SqlitePool,
    policy: RetentionPolicy,
    cancel: tokio_util::sync::CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        if policy.profile == Profile::None {
            return;
        }
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(6 * 60 * 60));
        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    tracing::info!("retention sweep shutting down");
                    return;
                }
                _ = interval.tick() => {}
            }
            tokio::select! {
                _ = cancel.cancelled() => {
                    tracing::info!("retention sweep cancelled mid-cycle; pending transaction will rollback");
                    return;
                }
                res = run_sweep(&pool, &policy) => {
                    if let Err(e) = res {
                        tracing::warn!(error = ?e, "retention sweep failed");
                    }
                }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-07-08 사용자 결정: `default` 프로파일을 CLI 기본으로 승격하면서
    /// 티어를 확정한다 — raw payload 60일(재도출 가능 기간; 30일은 파서 개선
    /// 재적용 창이 너무 짧다는 도그푸딩 판단), 분석 데이터(normalized·signal)
    /// 180일(대시보드 90일 프리셋의 2배 → 경계 뷰 완전 + 버퍼), audit 90일.
    #[test]
    fn default_profile_tiers_are_raw60_analyzable180_audit90() {
        assert_eq!(Profile::Default.raw_payload_days(), Some(60));
        assert_eq!(Profile::Default.normalized_event_days(), Some(180));
        assert_eq!(Profile::Default.insight_days(), Some(180));
        assert_eq!(Profile::Default.audit_days(), Some(90));
        // normalized == insight: 스윕이 둘을 한 패스로 지운다(테스트로 잠금 —
        // 어긋나면 별도 패스가 필요하다는 신호). run_sweep 주석 참조.
        assert_eq!(
            Profile::Default.normalized_event_days(),
            Profile::Default.insight_days()
        );
    }

    /// None/Strict는 이번 변경 대상이 아님 — 회귀 방지로 함께 잠근다.
    #[test]
    fn none_and_strict_tiers_unchanged() {
        assert_eq!(Profile::None.raw_payload_days(), None);
        assert_eq!(Profile::None.normalized_event_days(), None);
        assert_eq!(Profile::Strict.raw_payload_days(), Some(7));
        assert_eq!(Profile::Strict.normalized_event_days(), Some(30));
    }
}
