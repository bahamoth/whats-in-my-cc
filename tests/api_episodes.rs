//! Slice-12 — API endpoint tests for episodes.
//! (TDD red — Phase 1 commit 1.)
//!
//! Tests:
//! - `GET /v1/sessions/:id/episodes` — list (2 rows → 200 + data.len == 2)
//! - `GET /v1/episodes/:id` — detail (single row → 200 + phase valid)
//! - Empty session case (no episodes yet → 200 + data.len == 0)

use axum_test::TestServer;
use serde_json::Value;
use sqlx::sqlite::SqlitePoolOptions;
use witmcc::api::{router, AppState};
use witmcc::db::migrate;

async fn empty_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    pool
}

/// Inserts 2 episode rows directly into the DB for session "sess_t1".
async fn test_pool_with_seeded_episodes() -> sqlx::SqlitePool {
    let pool = empty_pool().await;
    sqlx::query(
        "INSERT INTO episode(
            episode_id, schema_version, session_id, phase,
            start_event_id, end_event_id, started_at, ended_at,
            evidence_node_ids, classification_basis, confidence,
            classifier_version)
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind("ep_001")
    .bind("episode.v1")
    .bind("sess_t1")
    .bind("intake")
    .bind("ev_000")
    .bind("ev_000")
    .bind("2026-05-27T10:00:00Z")
    .bind("2026-05-27T10:00:01Z")
    .bind("[]")
    .bind(r#"["phase_intake_fresh_user_message@v1"]"#)
    .bind(1.0f64)
    .bind("episode_classifier@v1")
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO episode(
            episode_id, schema_version, session_id, phase,
            start_event_id, end_event_id, started_at, ended_at,
            evidence_node_ids, classification_basis, confidence,
            classifier_version)
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind("ep_002")
    .bind("episode.v1")
    .bind("sess_t1")
    .bind("exploration")
    .bind("ev_001")
    .bind("ev_003")
    .bind("2026-05-27T10:00:02Z")
    .bind("2026-05-27T10:00:05Z")
    .bind("[]")
    .bind(r#"["phase_exploration_read_only_window@v1"]"#)
    .bind(0.85f64)
    .bind("episode_classifier@v1")
    .execute(&pool)
    .await
    .unwrap();

    pool
}

#[tokio::test]
async fn episodes_endpoint_returns_rows() {
    let pool = test_pool_with_seeded_episodes().await;
    let state = AppState::new_for_tests(pool);
    let server = TestServer::new(router(state)).unwrap();

    let r = server.get("/v1/sessions/sess_t1/episodes").await;
    r.assert_status_ok();
    let body: Value = r.json();
    assert_eq!(
        body["data"].as_array().unwrap().len(),
        2,
        "expected 2 episodes in response"
    );
    let p = body["data"][0]["phase"].as_str().unwrap();
    assert!(
        [
            "intake",
            "exploration",
            "diagnosis",
            "action",
            "verification",
            "repair",
            "drift"
        ]
        .contains(&p),
        "unexpected phase: {p}"
    );
}

#[tokio::test]
async fn episode_detail_returns_single_row() {
    let pool = test_pool_with_seeded_episodes().await;
    let state = AppState::new_for_tests(pool);
    let server = TestServer::new(router(state)).unwrap();

    let r = server.get("/v1/episodes/ep_001").await;
    r.assert_status_ok();
    let body: Value = r.json();
    assert_eq!(body["data"]["episode_id"].as_str().unwrap(), "ep_001");
    assert_eq!(body["data"]["phase"].as_str().unwrap(), "intake");
}

#[tokio::test]
async fn episodes_empty_session_returns_empty_array() {
    let pool = empty_pool().await;
    let state = AppState::new_for_tests(pool);
    let server = TestServer::new(router(state)).unwrap();

    let r = server.get("/v1/sessions/no_such_session/episodes").await;
    r.assert_status_ok();
    let body: Value = r.json();
    assert_eq!(
        body["data"].as_array().unwrap().len(),
        0,
        "expected empty array for unknown session"
    );
}
