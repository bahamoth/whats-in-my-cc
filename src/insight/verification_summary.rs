//! 대시보드 검증 탭 집계 (2026-07-04 전면 개편, 스펙 §3).
//!
//! 결정론 정의(테스트 SSOT: tests/api_verification_summary.rs):
//! - kind 매핑: `test_suite_*` → "test", `build`|`build_check` → "build",
//!   그 외 원문 유지 — 스키마에 없는 범주를 지어내지 않는다.
//! - recovered/abandoned: failed run과 같은 (session_id, command_kind)에
//!   `started_at`이 더 늦은 passed run이 있으면 recovered, 없으면 abandoned.
//! - rhythm pct: (run.started_at − session.first) / (last − first) × 100,
//!   소수 1자리. span 0 → 50.0.
//! - coverage(정밀, 2026-07-04 2차): hunk는 **도입 이벤트의 observed_at 이후에
//!   passed run이 존재할 때만** covered. 도입 시점을 알 수 없는 hunk는 커버로
//!   치지 않는다 — 검증 안 된 변경을 숨기지 않는 방향의 보수. per-run API의
//!   `covered_diff_hunk_ids`(routes.rs) 근사와 의미가 다름을 주석으로 명시.
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::SqlitePool;
use std::collections::BTreeMap;

use crate::db::{repo_diff_hunk, repo_observed, repo_verification_run};

/// list_sessions_filtered 후보 상한 — series::collect와 같은 취지의 방어선.
const CANDIDATE_CAP: i64 = 2000;
/// rhythm에 싣는 세션 수 — 승인 목업과 동일.
const RHYTHM_SESSIONS: usize = 4;
/// coverage by_session에 싣는 세션 수 — 승인 목업과 동일.
const COVERAGE_SESSIONS: usize = 6;

#[derive(Debug, Serialize)]
pub struct VerificationSummary {
    pub total: i64,
    pub measured: i64,
    pub passed: i64,
    pub failed: i64,
    pub unknown: i64,
    pub unknown_piped: i64,
    pub unknown_other: i64,
    pub not_executed: i64,
    pub by_kind: Vec<KindAgg>,
    pub failures: FailureAgg,
    pub rhythm: Vec<RhythmSession>,
    pub coverage: CoverageAgg,
}

#[derive(Debug, Serialize)]
pub struct KindAgg {
    pub kind: String,
    pub passed: i64,
    pub failed: i64,
    pub unknown: i64,
    pub not_executed: i64,
}

#[derive(Debug, Serialize)]
pub struct FailureAgg {
    pub recovered: i64,
    pub abandoned: i64,
}

#[derive(Debug, Serialize)]
pub struct RhythmSession {
    pub session_id: String,
    pub guards: i64,
    pub passed: i64,
    pub runs: Vec<RhythmRun>,
}

#[derive(Debug, Serialize)]
pub struct RhythmRun {
    pub pct: f64,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct CoverageAgg {
    pub covered: i64,
    pub total: i64,
    pub by_session: Vec<SessionCoverage>,
}

#[derive(Debug, Serialize)]
pub struct SessionCoverage {
    pub session_id: String,
    pub covered: i64,
    pub total: i64,
}

fn map_kind(raw: &str) -> String {
    if raw.starts_with("test_suite_") {
        "test".to_string()
    } else if raw == "build" || raw == "build_check" {
        "build".to_string()
    } else {
        raw.to_string()
    }
}

fn parse_ts(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .ok()
}

/// hunk 도입 이벤트들의 observed_at 조회 — 세션 단위 IN 쿼리 한 번.
async fn intro_timestamps(
    pool: &SqlitePool,
    session_id: &str,
    hunks: &[crate::db::repo_diff_hunk::DiffHunkRow],
) -> Result<std::collections::HashMap<String, DateTime<Utc>>> {
    use sqlx::Row as _;
    let ids: Vec<&str> = hunks
        .iter()
        .map(|h| h.introduced_by_event_id.as_str())
        .collect();
    if ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let placeholders = vec!["?"; ids.len()].join(",");
    let sql = format!(
        "SELECT event_id, observed_at FROM observed_event
         WHERE session_id = ? AND event_id IN ({placeholders})"
    );
    let mut q = sqlx::query(&sql).bind(session_id);
    for id in &ids {
        q = q.bind(*id);
    }
    let rows = q.fetch_all(pool).await?;
    let mut out = std::collections::HashMap::new();
    for row in rows {
        let ev: String = row.get("event_id");
        let at: String = row.get("observed_at");
        if let Some(ts) = parse_ts(&at) {
            out.insert(ev, ts);
        }
    }
    Ok(out)
}

/// 집계 입력 — (session_id, first_observed_at, last_observed_at).
struct SessionSpan {
    session_id: String,
    first_observed_at: String,
    last_observed_at: String,
}

pub async fn collect(
    pool: &SqlitePool,
    project: Option<&str>,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
) -> Result<VerificationSummary> {
    let sessions = repo_observed::list_sessions_filtered(pool, CANDIDATE_CAP, project).await?;
    let in_window = |first: &str| -> bool {
        let Some(ts) = parse_ts(first) else {
            return false;
        };
        from.is_none_or(|f| ts >= f) && to.is_none_or(|t| ts <= t)
    };
    let matched: Vec<SessionSpan> = sessions
        .into_iter()
        .filter(|s| in_window(&s.first_observed_at))
        .map(|s| SessionSpan {
            session_id: s.session_id,
            first_observed_at: s.first_observed_at,
            last_observed_at: s.last_observed_at,
        })
        .collect();
    aggregate(pool, &matched).await
}

/// §3c — 단일 세션 스코프. 미존재 세션은 빈 집계(0/빈 배열)로 응답한다.
pub async fn collect_session(pool: &SqlitePool, session_id: &str) -> Result<VerificationSummary> {
    let matched: Vec<SessionSpan> = match repo_observed::session_summary(pool, session_id).await? {
        Some((_count, first, last)) => vec![SessionSpan {
            session_id: session_id.to_string(),
            first_observed_at: first,
            last_observed_at: last,
        }],
        None => Vec::new(),
    };
    aggregate(pool, &matched).await
}

async fn aggregate(pool: &SqlitePool, matched: &[SessionSpan]) -> Result<VerificationSummary> {
    let mut total = 0i64;
    let mut passed = 0i64;
    let mut failed = 0i64;
    let mut unknown = 0i64;
    let mut unknown_piped = 0i64;
    let mut not_executed = 0i64;
    let mut by_kind: BTreeMap<String, KindAgg> = BTreeMap::new();
    let mut recovered = 0i64;
    let mut abandoned = 0i64;
    let mut rhythm_all: Vec<RhythmSession> = Vec::new();
    let mut cov_all: Vec<SessionCoverage> = Vec::new();

    for s in matched {
        let runs = repo_verification_run::list_session(pool, &s.session_id).await?;
        let hunks = repo_diff_hunk::list_session(pool, &s.session_id).await?;

        let mut sess_passed = 0i64;
        let mut passed_times: Vec<DateTime<Utc>> = Vec::new();
        for r in &runs {
            total += 1;
            let k = by_kind.entry(map_kind(&r.command_kind)).or_insert(KindAgg {
                kind: map_kind(&r.command_kind),
                passed: 0,
                failed: 0,
                unknown: 0,
                not_executed: 0,
            });
            match r.status.as_str() {
                "passed" => {
                    passed += 1;
                    sess_passed += 1;
                    k.passed += 1;
                    if let Some(ts) = parse_ts(&r.started_at) {
                        passed_times.push(ts);
                    }
                }
                "failed" => {
                    failed += 1;
                    k.failed += 1;
                    let rec = runs.iter().any(|p| {
                        p.status == "passed"
                            && p.command_kind == r.command_kind
                            && p.started_at > r.started_at
                    });
                    if rec {
                        recovered += 1;
                    } else {
                        abandoned += 1;
                    }
                }
                "not_executed" => {
                    not_executed += 1;
                    k.not_executed += 1;
                }
                _ => {
                    unknown += 1;
                    k.unknown += 1;
                    if r.status_basis == "piped" {
                        unknown_piped += 1;
                    }
                }
            }
        }

        // rhythm — 시간 위치. 세션 span은 observed_event min/max.
        let span = match (
            parse_ts(&s.first_observed_at),
            parse_ts(&s.last_observed_at),
        ) {
            (Some(a), Some(b)) if b > a => Some((a, (b - a).num_milliseconds() as f64)),
            (Some(a), Some(_)) => Some((a, 0.0)),
            _ => None,
        };
        if !runs.is_empty() {
            if let Some((start, span_ms)) = span {
                let mut rr: Vec<(String, RhythmRun)> = runs
                    .iter()
                    .filter_map(|r| {
                        let ts = parse_ts(&r.started_at)?;
                        let pct = if span_ms > 0.0 {
                            ((ts - start).num_milliseconds() as f64 / span_ms * 1000.0).round()
                                / 10.0
                        } else {
                            50.0
                        };
                        Some((
                            r.started_at.clone(),
                            RhythmRun {
                                pct: pct.clamp(0.0, 100.0),
                                status: r.status.clone(),
                            },
                        ))
                    })
                    .collect();
                rr.sort_by(|a, b| a.0.cmp(&b.0));
                rhythm_all.push(RhythmSession {
                    session_id: s.session_id.clone(),
                    guards: runs.len() as i64,
                    passed: sess_passed,
                    runs: rr.into_iter().map(|(_, r)| r).collect(),
                });
            }
        }

        if !hunks.is_empty() {
            // 정밀 커버리지: hunk 도입 이벤트의 observed_at을 조회해, 그 이후에
            // passed run이 있는 hunk만 covered로 센다.
            let intro_ts = intro_timestamps(pool, &s.session_id, &hunks).await?;
            let covered = hunks
                .iter()
                .filter(|h| {
                    intro_ts
                        .get(&h.introduced_by_event_id)
                        .is_some_and(|ts| passed_times.iter().any(|p| p >= ts))
                })
                .count() as i64;
            cov_all.push(SessionCoverage {
                session_id: s.session_id.clone(),
                covered,
                total: hunks.len() as i64,
            });
        }
    }

    // by_kind 정렬: 총수 desc, 동률은 kind asc(BTreeMap 순회가 이미 asc).
    let mut kinds: Vec<KindAgg> = by_kind.into_values().collect();
    kinds.sort_by(|a, b| {
        let ta = a.passed + a.failed + a.unknown + a.not_executed;
        let tb = b.passed + b.failed + b.unknown + b.not_executed;
        tb.cmp(&ta).then(a.kind.cmp(&b.kind))
    });

    rhythm_all.sort_by(|a, b| {
        b.guards
            .cmp(&a.guards)
            .then(a.session_id.cmp(&b.session_id))
    });
    rhythm_all.truncate(RHYTHM_SESSIONS);

    let covered_sum = cov_all.iter().map(|c| c.covered).sum();
    let total_sum = cov_all.iter().map(|c| c.total).sum();
    cov_all.sort_by(|a, b| b.total.cmp(&a.total).then(a.session_id.cmp(&b.session_id)));
    cov_all.truncate(COVERAGE_SESSIONS);

    Ok(VerificationSummary {
        total,
        measured: passed + failed,
        passed,
        failed,
        unknown,
        unknown_piped,
        unknown_other: unknown - unknown_piped,
        not_executed,
        by_kind: kinds,
        failures: FailureAgg {
            recovered,
            abandoned,
        },
        rhythm: rhythm_all,
        coverage: CoverageAgg {
            covered: covered_sum,
            total: total_sum,
            by_session: cov_all,
        },
    })
}
