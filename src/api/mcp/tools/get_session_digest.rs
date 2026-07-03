//! B-3 (2026-07-04) — MCP tool: whats_in_my_cc.get_session_digest
//!
//! 토큰 상한이 설계된 단일 콜: 전용 스킬 없이도 임의 에이전트가 "방금
//! 세션에서 뭐가 있었나"를 한 콜로 얻는다. 내용은 **순수 결정론 집계의
//! 조합**뿐이다(측정/판별 분리 — 판단 문장 없음): summary(개수·기간·
//! kind 분포) + fingerprint + SessionMetrics + signal 목록(절단은
//! total/returned로 노출, summary 문자열은 상한으로 절단 표기).
//! 드릴다운 경로는 links가 가리킨다(get_session_events/get_session_signals).

use serde_json::{json, Value};
use sqlx::SqlitePool;

use crate::api::mcp::tools::{tool_error, tool_success};
use crate::db::{repo_observed, repo_signal};
use crate::insight::fingerprint::compute_session_fingerprint;
use crate::insight::metrics::compute_session_metrics;

const DEFAULT_SIGNAL_LIMIT: usize = 20;
const MAX_SIGNAL_LIMIT: usize = 100;
/// signal summary 문자열 상한 — 다이제스트의 토큰 상한 설계의 일부.
const SUMMARY_MAX_CHARS: usize = 200;

fn cap_summary(s: &str) -> String {
    if s.chars().count() <= SUMMARY_MAX_CHARS {
        s.to_string()
    } else {
        s.chars().take(SUMMARY_MAX_CHARS).collect::<String>() + "…"
    }
}

pub async fn call(args: &Value, pool: &SqlitePool) -> Value {
    let Some(session_id) = args["session_id"].as_str().filter(|s| !s.is_empty()) else {
        return tool_error("session_id is required");
    };
    let signal_limit = args["signal_limit"]
        .as_u64()
        .map(|v| v as usize)
        .unwrap_or(DEFAULT_SIGNAL_LIMIT)
        .clamp(1, MAX_SIGNAL_LIMIT);

    let Some((event_count, first_observed_at, last_observed_at)) =
        (match repo_observed::session_summary(pool, session_id).await {
            Ok(s) => s,
            Err(e) => return tool_error(format!("db error: {e}")),
        })
    else {
        return tool_error(format!("session {session_id} not found"));
    };
    let by_kind = match repo_observed::session_kind_counts(pool, session_id).await {
        Ok(k) => k,
        Err(e) => return tool_error(format!("db error: {e}")),
    };
    let metrics = match compute_session_metrics(pool, session_id).await {
        Ok(m) => m,
        Err(e) => return tool_error(format!("metrics error: {e}")),
    };
    let fingerprint = match compute_session_fingerprint(pool, session_id).await {
        Ok(f) => f,
        Err(e) => return tool_error(format!("fingerprint error: {e}")),
    };
    let signals = match repo_signal::list_by_session(pool, session_id).await {
        Ok(s) => s,
        Err(e) => return tool_error(format!("db error: {e}")),
    };
    let total_signals = signals.len();
    let items: Vec<Value> = signals
        .into_iter()
        .take(signal_limit)
        .map(|s| {
            json!({
                "signal_id": s.signal_id,
                "detector": s.detector,
                "subkind": s.subkind,
                "summary": cap_summary(&s.summary),
                "created_at": s.created_at,
            })
        })
        .collect();

    tool_success(json!({ "data": {
        "session_id": session_id,
        "summary": {
            "event_count": event_count,
            "first_observed_at": first_observed_at,
            "last_observed_at": last_observed_at,
            "by_kind": by_kind,
        },
        "fingerprint": fingerprint,
        "metrics": metrics,
        "signals": {
            "total": total_signals,
            "returned": items.len(),
            "items": items,
        },
        "links": {
            "drill_down": ["whats_in_my_cc.get_session_events", "whats_in_my_cc.get_session_signals", "whats_in_my_cc.get_session_turns"],
            "replay_path": format!("/sessions/{session_id}"),
        },
    }}))
}
