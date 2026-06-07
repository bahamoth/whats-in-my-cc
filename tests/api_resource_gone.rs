//! Slice-19 (ported to signals, Plan 1) — 410 Gone for tombstoned resources.
//!
//! The signal_detail handler checks the retention tombstone table before the
//! live row, so callers can distinguish "expired by retention sweep" (410) from
//! "never existed" (404).

use axum_test::TestServer;

async fn build_auth_server_with_pool() -> (TestServer, sqlx::SqlitePool, String) {
    let token = wimcc::security::token::generate_token();

    let pool = wimcc::db::connect(":memory:").await.unwrap();
    wimcc::db::migrate(&pool).await.unwrap();

    let mut state = wimcc::api::AppState::new_for_tests(pool.clone());
    state.token = token.clone();

    let app = wimcc::api::router(state);
    let server = TestServer::new(app).unwrap();
    (server, pool, token)
}

#[tokio::test]
async fn pull_api_returns_410_for_tombstoned_signal() {
    let (server, pool, token) = build_auth_server_with_pool().await;

    // Insert a tombstone for a signal id that doesn't exist as a live row.
    sqlx::query(
        "INSERT INTO retention_tombstone (resource_id, resource_kind) VALUES ('sig_demo', 'signal')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let r = server
        .get("/v1/signals/sig_demo")
        .add_header(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {token}"),
        )
        .await;
    r.assert_status(axum::http::StatusCode::GONE);
}

#[tokio::test]
async fn pull_api_returns_404_for_nonexistent_nontombstoned_signal() {
    let (server, _pool, token) = build_auth_server_with_pool().await;

    let r = server
        .get("/v1/signals/sig_never_existed")
        .add_header(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {token}"),
        )
        .await;
    r.assert_status(axum::http::StatusCode::NOT_FOUND);
}
