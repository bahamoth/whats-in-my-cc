//! 세션 횡단 metrics series — HTTP `/v1/metrics`와 MCP `get_project_metrics`의
//! 공용 수집기. "프로젝트 P의 세션들에 대한 지표 추이/전후 비교"의 측정면이다.
//!
//! 판단(개선됐는가)은 소비자(LLM/사람) 몫 — 여기는 결정론 count와 환경
//! fingerprint만 제공한다(§6.3). 구현: list_sessions_filtered(최신순) →
//! first_observed_at 기간 필터 → limit 절단(절단 사실은 `matched_count`로
//! 노출 — silent cap 금지) → 세션별 on-demand metrics + fingerprint(§10.1).
//! 로컬 SQLite 단일 사용자 규모(세션 수십~수백)에서 충분하다.

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::SqlitePool;

use crate::db::repo_observed;
use crate::error::Result;
use crate::insight::fingerprint::{compute_session_fingerprint, SessionFingerprint};
use crate::insight::metrics::{compute_session_metrics, SessionMetrics};

pub const DEFAULT_LIMIT: i64 = 50;
pub const MAX_LIMIT: i64 = 200;
/// 후보 세션 수집 상한 — `/v1/sessions`의 5000 cap과 동일.
const CANDIDATE_CAP: i64 = 5000;

#[derive(Debug, Serialize)]
pub struct SessionSeriesRow {
    pub session_id: String,
    pub first_observed_at: String,
    pub last_observed_at: String,
    pub event_count: i64,
    pub metrics: SessionMetrics,
    pub fingerprint: SessionFingerprint,
}

#[derive(Debug, Serialize)]
pub struct SessionSeries {
    pub sessions: Vec<SessionSeriesRow>,
    /// 반환된 세션 수 (= sessions.len()).
    pub session_count: i64,
    /// limit 절단 전 필터 일치 세션 수 — 절단을 숨기지 않는다.
    pub matched_count: i64,
}

/// `from`/`to`는 세션의 first_observed_at(RFC3339) 기준 닫힌 구간 필터.
pub async fn collect(
    pool: &SqlitePool,
    project: Option<&str>,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    limit: i64,
) -> Result<SessionSeries> {
    let limit = limit.clamp(1, MAX_LIMIT);
    let rows = repo_observed::list_sessions_filtered(pool, CANDIDATE_CAP, project).await?;
    let in_window = |first: &str| -> bool {
        let Ok(ts) = DateTime::parse_from_rfc3339(first) else {
            // 저장 형식은 to_rfc3339 — 파싱 불가 행은 보수적으로 제외한다.
            return false;
        };
        let ts = ts.with_timezone(&Utc);
        from.is_none_or(|f| ts >= f) && to.is_none_or(|t| ts <= t)
    };
    let matched: Vec<_> = rows
        .into_iter()
        .filter(|r| in_window(&r.first_observed_at))
        .collect();
    let matched_count = matched.len() as i64;
    let mut sessions = Vec::new();
    for r in matched.into_iter().take(limit as usize) {
        let metrics = compute_session_metrics(pool, &r.session_id).await?;
        let fingerprint = compute_session_fingerprint(pool, &r.session_id).await?;
        sessions.push(SessionSeriesRow {
            session_id: r.session_id,
            first_observed_at: r.first_observed_at,
            last_observed_at: r.last_observed_at,
            event_count: r.event_count,
            metrics,
            fingerprint,
        });
    }
    Ok(SessionSeries {
        session_count: sessions.len() as i64,
        sessions,
        matched_count,
    })
}
