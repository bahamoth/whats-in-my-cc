//! Real-data anchoring: 동결된 session_facts_v01.jsonl(실 세션 6라인)이
//! SessionMetrics의 코퍼스 실측 fact 카운트 5종을 잠근다.
//!
//! fixture 구성 (모두 실 payload, 세션별):
//! - 0daafa6e: system/turn_duration (durationMs 139516, messageCount 100)
//! - 64c9ca09: system/api_error (status 529 overloaded_error)
//! - aac68973: system/compact_boundary (trigger manual, preTokens 310705)
//! - 3a1a90d0: tool_result 잘림 마커 "... [5882 characters truncated] ..."
//! - 77eb6194: user 중단 마커 2종 — "[Request interrupted by user for tool use]"
//!   / "[Request interrupted by user]"
//!
//! 코퍼스 실측(2026-06-10 재스캔, 776 파일): turn_duration 734 · api_error 12 ·
//! compact_boundary 44 · 잘림 마커 6 · 중단 마커 74 (44 plain + 30 for tool use).
//! `Command timed out` 마커는 현 코퍼스에 인용(mid-content) 22건만 존재하고
//! 하니스 생성 실 payload가 없어 anchoring 불가 — tool_timeout_count는 구현 보류.

use sqlx::sqlite::SqlitePoolOptions;
use wimcc::db::{migrate, repo_observed};
use wimcc::ingest::store;
use wimcc::insight::metrics::compute_session_metrics;
use wimcc::live::NoopSink;
use wimcc::model::observed::{EventKind, ObservedEvent};

const FIXTURE: &str = "tests/fixtures/transcripts/real/session_facts_v01.jsonl";

async fn ingested_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    store::ingest_file(&pool, std::path::Path::new(FIXTURE), &NoopSink)
        .await
        .expect("ingest");
    pool
}

async fn session(pool: &sqlx::SqlitePool, sid: &str) -> Vec<ObservedEvent> {
    repo_observed::list_session(pool, sid, 1000).await.unwrap()
}

fn find_subkind<'a>(events: &'a [ObservedEvent], subkind: &str) -> &'a ObservedEvent {
    events
        .iter()
        .find(|e| e.subkind.as_deref() == Some(subkind))
        .unwrap_or_else(|| panic!("event with subkind {subkind}"))
}

// ---------------------------------------------------------------------------
// Invariants — system 레코드 3종이 SystemSummary + subkind로 ingest되는 형태
// ---------------------------------------------------------------------------

#[tokio::test]
async fn real_turn_duration_record_shape() {
    let pool = ingested_pool().await;
    let evs = session(&pool, "0daafa6e-7a95-4c11-b76d-230208456833").await;
    let e = find_subkind(&evs, "turn_duration");
    assert_eq!(e.kind, EventKind::SystemSummary);
    // 실 payload: durationMs/messageCount는 system 레코드 top-level 숫자 필드.
    assert_eq!(
        e.payload.pointer("/durationMs").and_then(|v| v.as_i64()),
        Some(139_516)
    );
    assert_eq!(
        e.payload.pointer("/messageCount").and_then(|v| v.as_i64()),
        Some(100)
    );
}

#[tokio::test]
async fn real_api_error_record_shape() {
    let pool = ingested_pool().await;
    let evs = session(&pool, "64c9ca09-cc38-41e0-b896-7d16dafdb4ba").await;
    let e = find_subkind(&evs, "api_error");
    assert_eq!(e.kind, EventKind::SystemSummary);
    // 실 payload: error.status(HTTP) + 중첩 error.error.error.type(API error type).
    assert_eq!(
        e.payload.pointer("/error/status").and_then(|v| v.as_i64()),
        Some(529)
    );
    assert_eq!(
        e.payload
            .pointer("/error/error/error/type")
            .and_then(|v| v.as_str()),
        Some("overloaded_error")
    );
}

#[tokio::test]
async fn real_compact_boundary_record_shape() {
    let pool = ingested_pool().await;
    let evs = session(&pool, "aac68973-729e-4014-a02b-28a556f5ff29").await;
    let e = find_subkind(&evs, "compact_boundary");
    assert_eq!(e.kind, EventKind::SystemSummary);
    assert_eq!(
        e.payload
            .pointer("/compactMetadata/trigger")
            .and_then(|v| v.as_str()),
        Some("manual")
    );
    assert_eq!(
        e.payload
            .pointer("/compactMetadata/preTokens")
            .and_then(|v| v.as_i64()),
        Some(310_705)
    );
}

// ---------------------------------------------------------------------------
// Invariants — tool_result 잘림 마커 · user 중단 마커의 실 형태
// ---------------------------------------------------------------------------

#[tokio::test]
async fn real_truncation_marker_is_mid_content() {
    let pool = ingested_pool().await;
    let evs = session(&pool, "3a1a90d0-19fa-479b-930c-1e0e8b876fc7").await;
    let content = evs
        .iter()
        .find(|e| e.kind == EventKind::ToolResult)
        .and_then(|e| e.payload.pointer("/tool_result/content"))
        .and_then(|v| v.as_str())
        .expect("tool_result content");
    // 하니스 잘림 마커는 항상 본문 중간 — "\n\n... [N characters truncated] ...\n\n".
    assert!(content.contains("\n\n... [5882 characters truncated] ...\n\n"));
    assert!(!content.starts_with("... ["));
}

#[tokio::test]
async fn real_interruption_markers_are_exact_user_text() {
    let pool = ingested_pool().await;
    let evs = session(&pool, "77eb6194-53f8-494b-b6d9-a21494ccc0a2").await;
    let texts: Vec<&str> = evs
        .iter()
        .filter(|e| e.kind == EventKind::UserMessage)
        .filter_map(|e| e.payload.pointer("/text").and_then(|v| v.as_str()))
        .collect();
    // 두 변형 모두 text item 전체가 정확히 마커 문자열 (코퍼스 74건 전수 동일 형태).
    assert!(texts.contains(&"[Request interrupted by user]"));
    assert!(texts.contains(&"[Request interrupted by user for tool use]"));
}

// ---------------------------------------------------------------------------
// SessionMetrics — fixture 세션별 fact 카운트
// ---------------------------------------------------------------------------

#[tokio::test]
async fn metrics_count_session_facts_from_real_payloads() {
    let pool = ingested_pool().await;

    let m = compute_session_metrics(&pool, "0daafa6e-7a95-4c11-b76d-230208456833")
        .await
        .unwrap();
    assert_eq!(m.turn_duration_count, 1);
    assert_eq!(m.turn_duration_ms_total, 139_516);
    assert_eq!(m.api_error_count, 0);

    let m = compute_session_metrics(&pool, "64c9ca09-cc38-41e0-b896-7d16dafdb4ba")
        .await
        .unwrap();
    assert_eq!(m.api_error_count, 1);
    assert_eq!(m.compact_boundary_count, 0);

    let m = compute_session_metrics(&pool, "aac68973-729e-4014-a02b-28a556f5ff29")
        .await
        .unwrap();
    assert_eq!(m.compact_boundary_count, 1);

    let m = compute_session_metrics(&pool, "3a1a90d0-19fa-479b-930c-1e0e8b876fc7")
        .await
        .unwrap();
    assert_eq!(m.tool_result_truncated_count, 1);
    // 잘린 실패 출력은 disposition(미실행 축)이 아니다.
    assert_eq!(
        m.tool_user_rejected + m.tool_cancelled + m.tool_backgrounded,
        0
    );

    let m = compute_session_metrics(&pool, "77eb6194-53f8-494b-b6d9-a21494ccc0a2")
        .await
        .unwrap();
    assert_eq!(m.user_interruption_count, 2);
}
