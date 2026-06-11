//! Slice-19 (ported to signals, Plan 1) + full-retention (2026-06-11) —
//! 410 Gone for tombstoned resources.
//!
//! Handlers check the retention tombstone table before (or after missing) the
//! live row, so callers can distinguish "expired by retention sweep" (410)
//! from "never existed" (404). Covered classes: signal, raw payload
//! (`/v1/events/:id/raw`), session (detail + session-scoped lists),
//! verification_run.

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
        .add_header(axum::http::header::AUTHORIZATION, format!("Bearer {token}"))
        .await;
    r.assert_status(axum::http::StatusCode::GONE);
}

#[tokio::test]
async fn pull_api_returns_404_for_nonexistent_nontombstoned_signal() {
    let (server, _pool, token) = build_auth_server_with_pool().await;

    let r = server
        .get("/v1/signals/sig_never_existed")
        .add_header(axum::http::header::AUTHORIZATION, format!("Bearer {token}"))
        .await;
    r.assert_status(axum::http::StatusCode::NOT_FOUND);
}

/// Seed one raw_event + one observed_event child. Returns (raw_id, event_id).
async fn seed_event(pool: &sqlx::SqlitePool, session_id: &str) -> (String, String) {
    sqlx::query(
        "INSERT OR IGNORE INTO ingest_run (run_id, started_at, status) VALUES ('run_gone', datetime('now'), 'done')",
    )
    .execute(pool)
    .await
    .unwrap();
    let raw_id = format!("raw_{}", ulid::Ulid::new());
    sqlx::query(
        "INSERT INTO raw_event (raw_event_id, ingest_run_id, source_type, source_uri, source_line_no, source_byte_offset, payload_sha256, payload, captured_at)
         VALUES (?, 'run_gone', 'claude_transcript', 'gone.jsonl', 0, 0, ?, '{}', datetime('now'))",
    )
    .bind(&raw_id)
    .bind(format!("sha_{raw_id}"))
    .execute(pool)
    .await
    .unwrap();
    let event_id = format!("ev_{}", ulid::Ulid::new());
    sqlx::query(
        "INSERT INTO observed_event (event_id, raw_event_id, schema_version, session_id, observed_at, actor, kind, payload, parser_version)
         VALUES (?, ?, 'observed_event.v1', ?, datetime('now'), 'assistant', 'assistant_message', '{}', 'test')",
    )
    .bind(&event_id)
    .bind(&raw_id)
    .bind(session_id)
    .execute(pool)
    .await
    .unwrap();
    (raw_id, event_id)
}

async fn tombstone(pool: &sqlx::SqlitePool, id: &str, kind: &str) {
    sqlx::query("INSERT INTO retention_tombstone (resource_id, resource_kind) VALUES (?, ?)")
        .bind(id)
        .bind(kind)
        .execute(pool)
        .await
        .unwrap();
}

/// The raw payload class is the one the sweep actually scrubs: the skeleton
/// row survives, so the handler must consult the tombstone instead of serving
/// an empty payload as if it were the source record.
#[tokio::test]
async fn event_raw_returns_410_when_raw_payload_swept() {
    let (server, pool, token) = build_auth_server_with_pool().await;
    let (raw_id, event_id) = seed_event(&pool, "sess_gone_raw").await;
    tombstone(&pool, &raw_id, "raw_event").await;

    let r = server
        .get(&format!("/v1/events/{event_id}/raw"))
        .add_header(axum::http::header::AUTHORIZATION, format!("Bearer {token}"))
        .await;
    r.assert_status(axum::http::StatusCode::GONE);
}

#[tokio::test]
async fn event_raw_still_200_when_not_tombstoned() {
    let (server, pool, token) = build_auth_server_with_pool().await;
    let (_raw_id, event_id) = seed_event(&pool, "sess_live_raw").await;

    let r = server
        .get(&format!("/v1/events/{event_id}/raw"))
        .add_header(axum::http::header::AUTHORIZATION, format!("Bearer {token}"))
        .await;
    r.assert_status_ok();
}

#[tokio::test]
async fn session_detail_returns_410_for_tombstoned_session() {
    let (server, pool, token) = build_auth_server_with_pool().await;
    tombstone(&pool, "sess_expired", "session").await;

    let r = server
        .get("/v1/sessions/sess_expired")
        .add_header(axum::http::header::AUTHORIZATION, format!("Bearer {token}"))
        .await;
    r.assert_status(axum::http::StatusCode::GONE);

    // Never-existed sessions keep returning 404.
    let r = server
        .get("/v1/sessions/sess_never_existed")
        .add_header(axum::http::header::AUTHORIZATION, format!("Bearer {token}"))
        .await;
    r.assert_status(axum::http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn session_events_returns_410_for_tombstoned_session() {
    let (server, pool, token) = build_auth_server_with_pool().await;
    tombstone(&pool, "sess_expired", "session").await;

    let r = server
        .get("/v1/sessions/sess_expired/events")
        .add_header(axum::http::header::AUTHORIZATION, format!("Bearer {token}"))
        .await;
    r.assert_status(axum::http::StatusCode::GONE);
}

#[tokio::test]
async fn session_signals_returns_410_for_tombstoned_session() {
    let (server, pool, token) = build_auth_server_with_pool().await;
    tombstone(&pool, "sess_expired", "session").await;

    let r = server
        .get("/v1/sessions/sess_expired/signals")
        .add_header(axum::http::header::AUTHORIZATION, format!("Bearer {token}"))
        .await;
    r.assert_status(axum::http::StatusCode::GONE);
}

#[tokio::test]
async fn verification_run_detail_returns_410_when_tombstoned() {
    let (server, pool, token) = build_auth_server_with_pool().await;
    tombstone(&pool, "vr_expired", "verification_run").await;

    let r = server
        .get("/v1/verification-runs/vr_expired")
        .add_header(axum::http::header::AUTHORIZATION, format!("Bearer {token}"))
        .await;
    r.assert_status(axum::http::StatusCode::GONE);

    let r = server
        .get("/v1/verification-runs/vr_never_existed")
        .add_header(axum::http::header::AUTHORIZATION, format!("Bearer {token}"))
        .await;
    r.assert_status(axum::http::StatusCode::NOT_FOUND);
}
