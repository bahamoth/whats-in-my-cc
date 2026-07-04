//! instruction 관측 API — 스냅샷 내용 조회(diff 원료)와 세션 관측 목록.
use axum_test::TestServer;
use serde_json::Value;
use sqlx::sqlite::SqlitePoolOptions;
use wimcc::api::{router, AppState};
use wimcc::db::migrate;

async fn seeded() -> sqlx::SqlitePool {
    let p = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&p).await.unwrap();
    for (sha, content) in [("aa11", "# v1\n"), ("bb22", "# v2\n")] {
        sqlx::query(
            "INSERT INTO instruction_snapshot (content_sha256, content, first_observed_at) VALUES (?, ?, '2026-07-04T00:00:00+00:00')",
        )
        .bind(sha)
        .bind(content)
        .execute(&p)
        .await
        .unwrap();
    }
    for (id, sha, at) in [
        ("o1", "aa11", "2026-07-04T00:00:01+00:00"),
        ("o2", "bb22", "2026-07-04T01:00:00+00:00"),
    ] {
        sqlx::query(
            "INSERT INTO instruction_observation (observation_id, session_id, source, path, content_sha256, observed_at)
             VALUES (?, 's1', 'project', '/w/CLAUDE.md', ?, ?)",
        )
        .bind(id)
        .bind(sha)
        .bind(at)
        .execute(&p)
        .await
        .unwrap();
    }
    p
}

#[tokio::test]
async fn snapshot_content_by_hash() {
    let server = TestServer::new(router(AppState::new_for_tests(seeded().await))).unwrap();
    let r = server.get("/v1/instructions/aa11").await;
    r.assert_status_ok();
    let v: Value = r.json();
    assert_eq!(v["data"]["content"], "# v1\n");
    let missing = server.get("/v1/instructions/zzzz").await;
    missing.assert_status(axum::http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn session_observations_listed_in_time_order() {
    let server = TestServer::new(router(AppState::new_for_tests(seeded().await))).unwrap();
    let r = server.get("/v1/sessions/s1/instructions").await;
    r.assert_status_ok();
    let v: Value = r.json();
    let d = v["data"].as_array().unwrap();
    assert_eq!(d.len(), 2);
    assert_eq!(d[0]["content_sha256"], "aa11");
    assert_eq!(d[1]["content_sha256"], "bb22");
    assert_eq!(d[1]["source"], "project");
}
