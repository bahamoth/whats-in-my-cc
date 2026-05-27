//! Slice-9 L2 — `GET /v1/sessions/:id/events` cursor-paged endpoint.
//! Light 100-row seed so paging logic (page-1 + ?before=prev_cursor → page-2)
//! can be validated end-to-end through the HTTP layer. Heavy 10000-row paging
//! lives in `tests/sse_subprocess.rs` (Phase 5).

use axum_test::TestServer;
use chrono::{DateTime, TimeZone, Utc};
use serde_json::Value;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;

use witmcc::db::{migrate, repo_observed, repo_raw, repo_runs};
use witmcc::model::observed::{Actor, EventKind, ObservedEvent};

const SESS: &str = "sess-window";
const SEED_N: usize = 100;

async fn seed_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    let run_id = repo_runs::start(&pool).await.unwrap();
    let base: DateTime<Utc> = Utc.with_ymd_and_hms(2026, 5, 21, 0, 0, 0).unwrap();
    for i in 0..SEED_N {
        let event_id = format!("01J{:023}", i);
        let raw_id = format!("raw_{i:06}");
        repo_raw::insert_dedup(
            &pool,
            &repo_raw::NewRaw {
                raw_event_id: raw_id.clone(),
                ingest_run_id: run_id.clone(),
                source_type: "test".into(),
                source_uri: format!("test://{i}"),
                source_line_no: i as i64,
                source_byte_offset: 0,
                payload_sha256: format!("sha_{i:06}"),
                payload: b"{}".to_vec(),
                parse_error: None,
                captured_at: chrono::Utc::now(),
                redaction_state: "not_applicable".into(),
                redaction_manifest: None,
            },
        )
        .await
        .unwrap();
        let ev = ObservedEvent {
            event_id,
            raw_event_id: raw_id,
            schema_version: "0.5.0".into(),
            session_id: SESS.into(),
            observed_at: base + chrono::Duration::seconds(i as i64),
            actor: Actor::User,
            kind: EventKind::UserMessage,
            parser_version: "test".into(),
            ..Default::default()
        };
        repo_observed::insert(&pool, &ev).await.unwrap();
    }
    pool
}

async fn setup() -> TestServer {
    let pool = seed_pool().await;
    let app = witmcc::api::router(witmcc::api::AppState::new_for_tests(pool));
    TestServer::new(app).unwrap()
}

#[tokio::test]
async fn no_cursor_returns_newest_window_asc() {
    let s = setup().await;
    let v: Value = s
        .get("/v1/sessions/sess-window/events?limit=50")
        .await
        .json();
    let events = v["data"]["events"].as_array().unwrap();
    assert_eq!(events.len(), 50);
    // newest 50 of 100 = ids 50..=99 ASC
    assert_eq!(events.first().unwrap()["event_id"], format!("01J{:023}", 50));
    assert_eq!(events.last().unwrap()["event_id"], format!("01J{:023}", 99));
    // prev_cursor points to the oldest row in this page (consumer paginates older with it)
    assert!(v["data"]["prev_cursor"].is_string());
    // next_cursor is null because we're at the live tip
    assert!(v["data"]["next_cursor"].is_null());
}

#[tokio::test]
async fn before_cursor_paginates_backwards_no_overlap() {
    let s = setup().await;
    let page1: Value = s
        .get("/v1/sessions/sess-window/events?limit=50")
        .await
        .json();
    let prev = page1["data"]["prev_cursor"].as_str().unwrap();
    let page1_first_id = page1["data"]["events"][0]["event_id"].as_str().unwrap();
    let page2: Value = s
        .get(&format!(
            "/v1/sessions/sess-window/events?before={}&limit=50",
            urlencoding::encode(prev)
        ))
        .await
        .json();
    let events2 = page2["data"]["events"].as_array().unwrap();
    assert_eq!(events2.len(), 50);
    // page2 = ids 0..=49 ASC. page2_last < page1_first lexicographically and chronologically.
    assert_eq!(events2.first().unwrap()["event_id"], format!("01J{:023}", 0));
    assert_eq!(events2.last().unwrap()["event_id"], format!("01J{:023}", 49));
    // No overlap: page2_last comes strictly before page1_first.
    let page2_last_id = events2.last().unwrap()["event_id"].as_str().unwrap();
    assert!(page2_last_id < page1_first_id);
    // next_cursor non-null (we paginated backwards from live tip)
    assert!(page2["data"]["next_cursor"].is_string());
    // prev_cursor null because we hit the oldest row
    // (or it could be Some(<oldest>) — spec allows either; assert it's at least consistent)
}

#[tokio::test]
async fn after_cursor_paginates_forward() {
    let s = setup().await;
    // Build an after-cursor pointing at row 30. Strictly newer → ids 31..=80 ASC (50 rows).
    let after = format!("2026-05-21T00:00:30+00:00|01J{:023}", 30);
    let v: Value = s
        .get(&format!(
            "/v1/sessions/sess-window/events?after={}&limit=50",
            urlencoding::encode(&after)
        ))
        .await
        .json();
    let events = v["data"]["events"].as_array().unwrap();
    assert_eq!(events.len(), 50);
    assert_eq!(events.first().unwrap()["event_id"], format!("01J{:023}", 31));
    assert_eq!(events.last().unwrap()["event_id"], format!("01J{:023}", 80));
}

#[tokio::test]
async fn invalid_cursor_returns_400() {
    let s = setup().await;
    let resp = s
        .get("/v1/sessions/sess-window/events?before=garbage")
        .await;
    resp.assert_status(axum::http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn invalid_cursor_no_separator_returns_400() {
    let s = setup().await;
    let resp = s
        .get("/v1/sessions/sess-window/events?after=no-separator")
        .await;
    resp.assert_status(axum::http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn unknown_session_returns_empty_window_200() {
    let s = setup().await;
    let resp = s.get("/v1/sessions/no-such/events").await;
    resp.assert_status_ok();
    let v: Value = resp.json();
    assert!(v["data"]["events"].as_array().unwrap().is_empty());
    assert!(v["data"]["prev_cursor"].is_null());
    assert!(v["data"]["next_cursor"].is_null());
}

/// Slice-9 — session_detail must no longer carry events. WebUI fetches them
/// via `/events?...` separately. DEV-S8-14 cap on this endpoint is removed.
#[tokio::test]
async fn session_detail_does_not_return_events_field() {
    let s = setup().await;
    let v: Value = s.get("/v1/sessions/sess-window").await.json();
    // The events field is removed from the DTO. summary stays populated.
    let data = v["data"].as_object().unwrap();
    assert!(
        !data.contains_key("events"),
        "session_detail must not return events field — got {:?}",
        data.keys().collect::<Vec<_>>()
    );
    assert_eq!(data["summary"]["event_count"], 100);
}
