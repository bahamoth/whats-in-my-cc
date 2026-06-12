//! Dogfood 2026-06-12 — `GET /v1/sessions/:id/events?kind=` filter.
//!
//! Before this slice the param was silently ignored (axum Query drops unknown
//! fields), which misled API consumers into believing they had a filtered
//! window. Locked behaviours:
//! 1. `kind=<k>` returns only events of that kind, cursor-paged.
//! 2. `kind=a,b` (CSV) returns the union.
//! 3. Unknown kind value → 400 INVALID_KIND.
//! 4. Unknown query params → 400 (no more silent drops).
//! 5. Envelope unification: `meta` no longer carries the dead `next_cursor`
//!    field — pagination cursors live in `data` only (retrospect §3-4).

use axum_test::TestServer;
use chrono::{DateTime, TimeZone, Utc};
use serde_json::Value;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;

use wimcc::db::{migrate, repo_observed, repo_raw, repo_runs};
use wimcc::model::observed::{Actor, EventKind, ObservedEvent};

const SESS: &str = "sess-kind";

/// 30 user_message + 30 tool_call + 30 metric_sample, interleaved in time.
async fn seed_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    let run_id = repo_runs::start(&pool).await.unwrap();
    let base: DateTime<Utc> = Utc.with_ymd_and_hms(2026, 6, 12, 0, 0, 0).unwrap();
    let kinds = [
        EventKind::UserMessage,
        EventKind::ToolCall,
        EventKind::MetricSample,
    ];
    for i in 0..90usize {
        let event_id = format!("01K{i:023}");
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
            kind: kinds[i % 3],
            parser_version: "test".into(),
            ..Default::default()
        };
        repo_observed::insert(&pool, &ev).await.unwrap();
    }
    pool
}

async fn setup() -> TestServer {
    let pool = seed_pool().await;
    let app = wimcc::api::router(wimcc::api::AppState::new_for_tests(pool));
    TestServer::new(app).unwrap()
}

#[tokio::test]
async fn kind_filter_returns_only_that_kind() {
    let s = setup().await;
    let v: Value = s
        .get(&format!(
            "/v1/sessions/{SESS}/events?kind=tool_call&limit=10"
        ))
        .await
        .json();
    let events = v["data"]["events"].as_array().unwrap();
    assert_eq!(events.len(), 10);
    assert!(events.iter().all(|e| e["kind"] == "tool_call"));
    // newest 10 tool_calls ASC on the wire: indices 61..=88 step 3
    assert_eq!(events.last().unwrap()["event_id"], format!("01K{:023}", 88));
    // newest-anchored window ⇒ at the filtered live tip ⇒ no next page.
    assert!(v["data"]["next_cursor"].is_null());
    assert!(v["data"]["prev_cursor"].is_string());
}

#[tokio::test]
async fn kind_filter_paginates_backwards_with_before() {
    let s = setup().await;
    let page1: Value = s
        .get(&format!(
            "/v1/sessions/{SESS}/events?kind=tool_call&limit=10"
        ))
        .await
        .json();
    let prev = page1["data"]["prev_cursor"].as_str().unwrap();
    let page2: Value = s
        .get(&format!(
            "/v1/sessions/{SESS}/events?kind=tool_call&limit=10&before={}",
            urlencoding::encode(prev)
        ))
        .await
        .json();
    let events2 = page2["data"]["events"].as_array().unwrap();
    assert_eq!(events2.len(), 10);
    assert!(events2.iter().all(|e| e["kind"] == "tool_call"));
    let page1_first = page1["data"]["events"][0]["event_id"].as_str().unwrap();
    let page2_last = events2.last().unwrap()["event_id"].as_str().unwrap();
    assert!(page2_last < page1_first, "no overlap between pages");
}

#[tokio::test]
async fn kind_filter_csv_returns_union() {
    let s = setup().await;
    let v: Value = s
        .get(&format!(
            "/v1/sessions/{SESS}/events?kind=user_message,tool_call&limit=60"
        ))
        .await
        .json();
    let events = v["data"]["events"].as_array().unwrap();
    assert_eq!(events.len(), 60);
    assert!(events
        .iter()
        .all(|e| e["kind"] == "user_message" || e["kind"] == "tool_call"));
}

#[tokio::test]
async fn unknown_kind_value_returns_400() {
    let s = setup().await;
    let resp = s
        .get(&format!("/v1/sessions/{SESS}/events?kind=banana"))
        .await;
    resp.assert_status(axum::http::StatusCode::BAD_REQUEST);
    let v: Value = resp.json();
    assert_eq!(v["title"], "INVALID_KIND");
}

#[tokio::test]
async fn unknown_query_param_returns_400() {
    // Pre-slice behaviour: `?bogus=` was silently ignored and the caller got
    // an unfiltered window — the exact failure mode hit during dogfooding.
    let s = setup().await;
    let resp = s.get(&format!("/v1/sessions/{SESS}/events?bogus=1")).await;
    resp.assert_status(axum::http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn meta_has_no_next_cursor_field() {
    // Retrospect §3-4 — the envelope-level next_cursor was dead (always null)
    // while the real cursor lives in data; the dead field is removed.
    let s = setup().await;
    let v: Value = s.get(&format!("/v1/sessions/{SESS}/events")).await.json();
    assert!(
        v["meta"].as_object().unwrap().get("next_cursor").is_none(),
        "meta must not carry next_cursor — pagination cursors live in data"
    );
}
