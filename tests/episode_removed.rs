//! Regression guard for the episode/phase removal (2026-05-31).
//!
//! The episode side-table, its repo, the L1 `missing_verification` extractor,
//! and the two episode HTTP routes were removed. Migration 0017 drops the
//! `episode` table. These tests lock that removal:
//!   - `GET /v1/sessions/:id/episodes` is no longer an API route
//!   - `GET /v1/episodes/:id`          is no longer an API route
//!   - a freshly-migrated DB has no `episode` table.
//!
//! Note on the route assertions: the router has a SPA `.fallback`
//! (`static_assets::spa_handler`) that serves `index.html` (200 +
//! `text/html`) for *any* path not matched by a registered route. So a removed
//! API route does NOT 404 — it falls through to the SPA and returns HTML.
//! The genuine signal that the route is gone is therefore "served by the SPA
//! fallback (text/html), not by a JSON API handler". We anchor this by
//! contrasting with a live API route (`/v1/health`) that still returns JSON.
//!
//! Harness mirrors `tests/api.rs` (TestServer + router + new_for_tests) and
//! `tests/db_init.rs` (in-memory pool + migrate).

use axum_test::TestServer;
use sqlx::sqlite::SqlitePoolOptions;
use witmcc::db::migrate;

async fn make_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    pool
}

async fn setup() -> TestServer {
    let pool = make_pool().await;
    let app = witmcc::api::router(witmcc::api::AppState::new_for_tests(pool));
    TestServer::new(app).unwrap()
}

/// Content-type of a response, lower-cased, or empty string if absent.
fn content_type(resp: &axum_test::TestResponse) -> String {
    resp.headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase()
}

#[tokio::test]
async fn session_episodes_route_is_gone() {
    let s = setup().await;

    // A live API route still answers with JSON — proves the server/router work.
    let live = s.get("/v1/health").await;
    live.assert_status_ok();
    assert!(
        content_type(&live).contains("application/json"),
        "live API route must return JSON; got {:?}",
        content_type(&live)
    );

    // The removed episode list route is not registered: it falls through to the
    // SPA fallback and is served as HTML, never as a JSON API response.
    let gone = s.get("/v1/sessions/sess_any/episodes").await;
    assert!(
        content_type(&gone).contains("text/html"),
        "removed /v1/sessions/:id/episodes must hit the SPA fallback (text/html), \
         not a JSON API handler; got {:?}",
        content_type(&gone)
    );
    assert!(
        !content_type(&gone).contains("application/json"),
        "removed episode list route must not be served by a JSON API handler"
    );
}

#[tokio::test]
async fn episode_detail_route_is_gone() {
    let s = setup().await;

    let gone = s.get("/v1/episodes/ep_any").await;
    assert!(
        content_type(&gone).contains("text/html"),
        "removed /v1/episodes/:id must hit the SPA fallback (text/html), \
         not a JSON API handler; got {:?}",
        content_type(&gone)
    );
    assert!(
        !content_type(&gone).contains("application/json"),
        "removed episode detail route must not be served by a JSON API handler"
    );
}

#[tokio::test]
async fn episode_table_dropped_by_migration() {
    let pool = make_pool().await;
    let (count,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='episode'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 0, "episode table must not exist after migration 0017");
}
