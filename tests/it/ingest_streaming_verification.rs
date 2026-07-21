//! 스트리밍 ingest에서 verification_run 고아 행 방지 검증.
//!
//! 워처는 transcript가 자라는 중간에도 ingest한다 — tool_call 라인은 도착했지만
//! tool_result가 아직 없는 슬라이스에서 recompute가 돌면 trigger=tool_call 이벤트
//! 로 unknown 행이 생긴다. result 도착 후 재추출 행은 trigger=tool_result라서
//! vr_id(sha256(session||trigger||started_at))가 달라져 INSERT OR REPLACE(PK)로는
//! 이전 행이 지워지지 않았다.
//!
//! 실사고 2026-07-06 (세션 bebd8197): unknown 36건 전부 ended_at=None 고아 —
//! `f_verification=unknown` 필터에 완료된 run의 tool_call 이벤트가 유령 매칭됐다.
//! 수정: recompute가 세션 추출 산출 전체를 원자 교체(replace_session).
use sqlx::sqlite::SqlitePoolOptions;
use wimcc::db::{migrate, repo_verification_run};
use wimcc::ingest::store;
use wimcc::live::NoopSink;

async fn empty_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    pool
}

#[tokio::test]
async fn result_slice_replaces_call_only_orphan_run() {
    let pool = empty_pool().await;
    let full =
        std::fs::read_to_string("tests/fixtures/transcripts/real/verification_v01.jsonl").unwrap();
    let lines: Vec<&str> = full.lines().collect();

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("grow.jsonl");

    // 슬라이스 1: 첫 Bash 검증 tool_call만 (result 미도착 — 자라는 파일의 중간 상태).
    std::fs::write(&path, format!("{}\n", lines[0])).unwrap();
    let stats = store::ingest_paths(&pool, &[path.clone()], &NoopSink)
        .await
        .unwrap();
    let sid = stats.sessions_touched.iter().next().unwrap().clone();
    let runs = repo_verification_run::list_session(&pool, &sid)
        .await
        .unwrap();
    assert_eq!(runs.len(), 1, "call-only slice emits one in-flight run");
    assert!(runs[0].ended_at.is_none());

    // 슬라이스 2: 파일이 다 자란 상태(전체 6줄 = call/result 3쌍) 재-ingest.
    std::fs::write(&path, &full).unwrap();
    store::ingest_paths(&pool, &[path], &NoopSink)
        .await
        .unwrap();
    let runs = repo_verification_run::list_session(&pool, &sid)
        .await
        .unwrap();
    assert_eq!(
        runs.len(),
        3,
        "재추출은 세션 산출을 교체해야 한다 — call-키 고아가 남으면 4행이 된다; got {:?}",
        runs.iter()
            .map(|r| (r.trigger_event_id.clone(), r.status.clone()))
            .collect::<Vec<_>>()
    );
    assert!(
        runs.iter().all(|r| r.ended_at.is_some()),
        "완료된 세션에 ended_at=None 고아 행이 남으면 안 된다"
    );
    // tool_use_id당 정확히 1행 — call-키/result-키 이중 매핑이
    // f_verification 필터의 유령 매칭(unknown)을 만든 실사고의 직접 잠금.
    let mut tids: Vec<_> = runs
        .iter()
        .filter_map(|r| r.trigger_tool_use_id.clone())
        .collect();
    tids.sort();
    tids.dedup();
    assert_eq!(tids.len(), 3, "one run per tool_use_id");
}
