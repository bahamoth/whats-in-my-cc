//! /v1/health must include status "ok" and a security block.
//! The judge counters were removed when the LLM judge subsystem was deleted.

use axum_test::TestServer;
use wimcc::api::AppState;

async fn test_server() -> TestServer {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&pool)
        .await
        .unwrap();
    wimcc::db::migrate(&pool).await.unwrap();
    TestServer::new(wimcc::api::router(AppState::new_for_tests(pool))).unwrap()
}

#[tokio::test]
async fn health_returns_ok() {
    let srv = test_server().await;
    let r = srv.get("/v1/health").await;
    r.assert_status_ok();
    let body: serde_json::Value = r.json();
    assert_eq!(body["status"], "ok", "status must be 'ok'");
}

#[tokio::test]
async fn health_includes_security_block() {
    let srv = test_server().await;
    let r = srv.get("/v1/health").await;
    let body: serde_json::Value = r.json();
    assert!(
        body["security"].is_object(),
        "security block missing from health response"
    );
    assert_eq!(
        body["security"]["auth_required"], false,
        "auth_required must be false in test mode (empty token)"
    );
}

/// 스펙 2026-07-17 §4 — health에 version 블록. 테스트 서버는 체크 루프를
/// 돌리지 않으므로 latest는 미조회 = null이어야 한다.
#[tokio::test]
async fn health_includes_version_block() {
    let srv = test_server().await;
    let body: serde_json::Value = srv.get("/v1/health").await.json();
    assert_eq!(body["version"]["current"], env!("CARGO_PKG_VERSION"));
    assert!(body["version"]["update_available"].is_boolean());
    assert!(body["version"]["latest"].is_null());
}

/// growth-2026-07-18 — 운영자가 DB가 얼마나 큰지 볼 방법이 row count 추정뿐
/// 이었다(1.2GB 도그푸딩 DB를 파일시스템에서야 발견). page 기반 산출이라
/// 파일·메모리 DB 공통으로 동작한다.
#[tokio::test]
async fn health_includes_db_block_with_size() {
    let srv = test_server().await;
    let body: serde_json::Value = srv.get("/v1/health").await.json();
    let size = body["db"]["size_bytes"]
        .as_i64()
        .unwrap_or_else(|| panic!("db.size_bytes missing: {body}"));
    assert!(size > 0, "migrated DB must report a positive size; got {size}");
    assert!(
        body["db"]["freelist_bytes"].as_i64().is_some(),
        "db.freelist_bytes missing: {body}"
    );
    assert!(
        body["db"]["path"].is_null(),
        "test state has no resolved db path"
    );
}

/// growth-2026-07-18 — SweepStats는 정의만 있고 어디에도 배선되지 않은
/// dead code였다(마지막 sweep 시각·삭제량을 볼 곳이 audit row뿐).
/// 공유 핸들을 sweep task가 쓰고 health가 읽는다.
#[tokio::test]
async fn health_reports_last_sweep_after_stats_update() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    wimcc::db::migrate(&pool).await.unwrap();
    let state = AppState::new_for_tests(pool);
    let handle = state.sweep_stats.clone();
    let srv = TestServer::new(wimcc::api::router(state)).unwrap();

    let body: serde_json::Value = srv.get("/v1/health").await.json();
    assert!(
        body["retention"]["last_sweep_at"].is_null(),
        "no sweep has run yet: {body}"
    );

    {
        let mut s = handle.write().await;
        s.last_sweep_at = Some("2026-07-18T00:00:00+00:00".into());
        s.last_sweep_deletions =
            std::collections::HashMap::from([("raw_event".to_string(), 3u64)]);
    }
    let body: serde_json::Value = srv.get("/v1/health").await.json();
    assert_eq!(body["retention"]["last_sweep_at"], "2026-07-18T00:00:00+00:00");
    assert_eq!(body["retention"]["last_sweep_deletions"]["raw_event"], 3);
}
