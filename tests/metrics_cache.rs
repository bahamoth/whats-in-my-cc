//! B-8 (2026-07-04) — SessionMetrics 인메모리 캐시.
//!
//! §10.1 게이트 실측(2026-07-04, 스크래치 DB): 6003-이벤트 세션의
//! compute_session_metrics ≈ 232ms/콜, /v1/metrics series(18세션) ≈ 1.2s.
//! B-1 대시보드가 series를 인터랙티브 경로로 만들면서 호출 빈도 조건이
//! 충족됐다. 캐시 키는 (event_count, last_observed_at) — append-only ingest
//! 에서 데이터 변화는 반드시 키를 바꾼다. detector 재구성으로 signal만
//! 바뀌는 경로는 새 이벤트(재ingest flush) 또는 프로세스 재시작을 동반한다
//! (인메모리 캐시라 재시작에 함께 사라짐 — 스테일 불가).

use sqlx::sqlite::SqlitePoolOptions;
use wimcc::db::migrate;
use wimcc::ingest::store;
use wimcc::insight::metrics::{cache_hits, compute_session_metrics};
use wimcc::live::NoopSink;

#[tokio::test]
async fn second_compute_hits_cache_and_ingest_invalidates() {
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    store::ingest_file(
        &pool,
        std::path::Path::new("tests/fixtures/transcripts/minimal_session.jsonl"),
        &wimcc::live::NoopSink,
    )
    .await
    .unwrap();

    let m1 = compute_session_metrics(&pool, "sess-A").await.unwrap();
    let hits_before = cache_hits();
    let m2 = compute_session_metrics(&pool, "sess-A").await.unwrap();
    assert!(
        cache_hits() > hits_before,
        "second call must be a cache hit"
    );
    assert_eq!(m1.tool_call_total, m2.tool_call_total);

    // 새 이벤트가 ingest되면(키 변화) 캐시가 스테일을 내놓지 않는다.
    let extra = r#"{"type":"user","uuid":"u9","parentUuid":"a2","sessionId":"sess-A","timestamp":"2026-05-19T04:00:00Z","cwd":"/tmp","gitBranch":"main","entrypoint":"cli","userType":"external","version":"2.1.144","isSidechain":false,"isMeta":false,"promptId":"p9","message":{"role":"user","content":[{"type":"text","text":"[Request interrupted by user]"}]}}"#;
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("extra.jsonl");
    std::fs::write(&f, format!("{extra}\n")).unwrap();
    store::ingest_file(&pool, &f, &NoopSink).await.unwrap();

    let m3 = compute_session_metrics(&pool, "sess-A").await.unwrap();
    assert_eq!(
        m3.user_interruption_count,
        m1.user_interruption_count + 1,
        "cache must not serve stale metrics after new events"
    );
}
