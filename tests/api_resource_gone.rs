//! Slice-19 — Red-locking tests: 410 Gone for tombstoned resources.
//!
//! These tests will FAIL until the tombstone check is wired into
//! finding_detail (and related) handlers.

use axum_test::TestServer;

async fn build_auth_server_with_pool() -> (TestServer, sqlx::SqlitePool, String) {
    let token = witmcc::security::token::generate_token();

    let pool = witmcc::db::connect(":memory:").await.unwrap();
    witmcc::db::migrate(&pool).await.unwrap();

    let mut state = witmcc::api::AppState::new_for_tests(pool.clone());
    state.token = token.clone();

    let app = witmcc::api::router(state);
    let server = TestServer::new(app).unwrap();
    (server, pool, token)
}

#[tokio::test]
async fn pull_api_returns_410_for_tombstoned_finding() {
    let (server, pool, token) = build_auth_server_with_pool().await;

    // Insert a tombstone for a finding id that doesn't exist as a live row.
    sqlx::query(
        "INSERT INTO retention_tombstone (resource_id, resource_kind) VALUES ('find_demo', 'finding')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let r = server
        .get("/v1/findings/find_demo")
        .add_header(
            axum::http::header::AUTHORIZATION,
            &format!("Bearer {token}"),
        )
        .await;
    r.assert_status(axum::http::StatusCode::GONE);
}

#[tokio::test]
async fn pull_api_returns_404_for_nonexistent_nontombstoned_finding() {
    let (server, _pool, token) = build_auth_server_with_pool().await;

    let r = server
        .get("/v1/findings/find_never_existed")
        .add_header(
            axum::http::header::AUTHORIZATION,
            &format!("Bearer {token}"),
        )
        .await;
    r.assert_status(axum::http::StatusCode::NOT_FOUND);
}
