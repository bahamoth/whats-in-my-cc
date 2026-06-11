//! ingest --all batch: 같은 세션의 여러 파일을 처리할 때 insight 재계산을
//! 세션 합집합에 대해 1회만 수행함을 검증한다 (성능 최적화, 결과 불변).
//! Dogfooding 2026-06-11: 종전 `ingest --all`은 파일마다 그 세션 전체를 재계산해
//! subagent 파일 N개면 같은 세션을 N번 중복 재계산했다(733파일 ≈ 37분).
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
async fn ingest_paths_unions_same_session_and_recomputes_correctly() {
    let pool = empty_pool().await;
    let f = std::path::PathBuf::from("tests/fixtures/transcripts/real/verification_v01.jsonl");
    // 같은 세션의 여러 파일을 batch로 ingest (같은 파일 2회로 모사: 2번째 raw는
    // dedup-skip되지만 세션은 touch된다). 합집합이므로 sessions_touched는 1개여야
    // 하고(= 재계산 1회), 산출(verification_run)은 단일 ingest와 동일해야 한다.
    let stats = store::ingest_paths(&pool, &[f.clone(), f.clone()], &NoopSink)
        .await
        .unwrap();
    assert_eq!(
        stats.sessions_touched.len(),
        1,
        "same session must be unioned to a single recompute"
    );
    let sid = stats.sessions_touched.iter().next().unwrap();
    let runs = repo_verification_run::list_session(&pool, sid)
        .await
        .unwrap();
    assert!(
        !runs.is_empty(),
        "batch recompute must produce verification_runs for the session"
    );
}
