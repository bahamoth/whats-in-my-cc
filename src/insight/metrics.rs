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
use crate::insight::disposition::{classify_disposition, Disposition};
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
    /// `tool_result`가 사용자 거부 마커로 시작한 호출 수 (실행되지 않음 —
    /// 실패도 unknown도 아닌 별도 축; `disposition::classify_disposition`).
    pub tool_user_rejected: i64,
    /// PreToolUse hook이 차단한 호출 수 ("Hook …denied this tool").
    pub tool_policy_denied: i64,
    /// 병렬 tool call 취소 수 ("<tool_use_error>Cancelled: …").
    pub tool_cancelled: i64,
    /// 백그라운드 실행으로 전환된 호출 수 — 해당 tool_result content는 실제
    /// 출력이 아니므로 출력 기반 분류에서 제외해야 한다.
    pub tool_backgrounded: i64,
    /// system/turn_duration 레코드의 `durationMs` 합 (밀리초). 분모는
    /// `turn_duration_count` — 평균은 소비자가 계산한다 (F1: count/합만).
    pub turn_duration_ms_total: i64,
    /// system/turn_duration 레코드 수 (= 하니스가 측정한 turn 수).
    pub turn_duration_count: i64,
    /// system/api_error 레코드 수 (API 요청 실패·재시도 이벤트).
    pub api_error_count: i64,
    /// system/compact_boundary 레코드 수 (컨텍스트 압축 발생 횟수).
    pub compact_boundary_count: i64,
    /// 하니스 잘림 마커("… [N characters truncated] …")를 포함한 tool_result 수
    /// — 출력이 잘려 보존된 호출의 fact.
    pub tool_result_truncated_count: i64,
    /// "[Request interrupted by user]" / "… for tool use]" 마커 user_message 수
    /// — 사용자가 turn/도구 실행을 중단한 횟수.
    pub user_interruption_count: i64,
    /// detector → number of signals fired (signal distribution, spec §6.6).
    pub detector_firing: BTreeMap<String, i64>,
}

/// 하니스 출력 잘림 마커 — `... [N characters truncated] ...` (N = 자릿수).
///
/// Real-data anchoring: `session_facts_v01.jsonl` 동결 payload + 코퍼스 실측
/// 6건 전수에서 마커는 항상 본문 *중간*에 `\n\n` 으로 둘러싸여 나타나므로
/// prefix 매칭이 불가능해 substring 매칭을 쓴다. 숫자 자리는 가변이라
/// 숫자 1개 이상을 요구한다 — 문서 인용 형태(`[N characters truncated]`,
/// 리터럴 N)는 매칭되지 않는다.
fn has_truncation_marker(content: &str) -> bool {
    const HEAD: &str = "... [";
    const TAIL: &str = " characters truncated] ...";
    let mut rest = content;
    while let Some(i) = rest.find(HEAD) {
        let after = &rest[i + HEAD.len()..];
        let digits = after.bytes().take_while(u8::is_ascii_digit).count();
        if digits > 0 && after[digits..].starts_with(TAIL) {
            return true;
        }
        rest = after;
    }
    false
}

/// 사용자 중단 마커 — user_message text가 마커로 시작하는가.
///
/// 실측 두 변형 `[Request interrupted by user]` / `[Request interrupted by
/// user for tool use]` (코퍼스 74건 전수에서 text item *전체*가 정확히 마커
/// 문자열). disposition과 같은 prefix 기준이라 본문 중간 인용은 매칭되지 않는다.
fn is_interruption_marker(text: &str) -> bool {
    text.starts_with("[Request interrupted by user")
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

    // 실행되지 않은/중단된 호출의 결정론 분류 (disposition.rs — 하니스 마커
    // prefix 매칭, real fixture로 잠김). 실패(tool_failure)·unknown과 별도 축.
    let (mut tool_user_rejected, mut tool_policy_denied, mut tool_cancelled, mut tool_backgrounded) =
        (0i64, 0i64, 0i64, 0i64);
    // 코퍼스 실측 session fact 카운트 (session_facts_v01.jsonl로 잠김).
    let (mut turn_duration_ms_total, mut turn_duration_count) = (0i64, 0i64);
    let (mut api_error_count, mut compact_boundary_count) = (0i64, 0i64);
    let (mut tool_result_truncated_count, mut user_interruption_count) = (0i64, 0i64);
    for e in &events {
        match e.kind {
            EventKind::ToolResult => {
                let Some(content) = e
                    .payload
                    .pointer("/tool_result/content")
                    .and_then(|v| v.as_str())
                else {
                    continue;
                };
                match classify_disposition(content) {
                    Some(Disposition::UserRejected) => tool_user_rejected += 1,
                    Some(Disposition::PolicyDenied) => tool_policy_denied += 1,
                    Some(Disposition::Cancelled) => tool_cancelled += 1,
                    Some(Disposition::Backgrounded) => tool_backgrounded += 1,
                    None => {}
                }
                if has_truncation_marker(content) {
                    tool_result_truncated_count += 1;
                }
            }
            EventKind::SystemSummary => match e.subkind.as_deref() {
                Some("turn_duration") => {
                    turn_duration_count += 1;
                    // 실 payload(system 레코드 top-level)의 durationMs. 필드가
                    // 없는 변형은 코퍼스에서 관측되지 않았다 — 방어적 0 가산.
                    turn_duration_ms_total += e
                        .payload
                        .pointer("/durationMs")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0);
                }
                Some("api_error") => api_error_count += 1,
                Some("compact_boundary") => compact_boundary_count += 1,
                _ => {}
            },
            EventKind::UserMessage => {
                // 중단 마커는 array-content의 text item으로만 실측됨
                // (코퍼스 74건 전수) — user_message payload의 /text를 본다.
                if e.payload
                    .pointer("/text")
                    .and_then(|v| v.as_str())
                    .is_some_and(is_interruption_marker)
                {
                    user_interruption_count += 1;
                }
            }
            _ => {}
        }
    }

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
        tool_user_rejected,
        tool_policy_denied,
        tool_cancelled,
        tool_backgrounded,
        turn_duration_ms_total,
        turn_duration_count,
        api_error_count,
        compact_boundary_count,
        tool_result_truncated_count,
        user_interruption_count,
        detector_firing,
    })
}
