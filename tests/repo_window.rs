//! Slice-9 L1 tests — `repo_observed::list_session_window` cursor-paged
//! reads. Seed 1500 rows so we can exercise limit clamp (>1000) and three
//! cursor combinations (no-cursor, before, after) with strict ordering.

use chrono::{DateTime, TimeZone, Utc};
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;

use witmcc::db::{migrate, repo_observed, repo_raw, repo_runs};
use witmcc::model::cursor::Cursor;
use witmcc::model::observed::{Actor, EventKind, ObservedEvent};

const SESS: &str = "sess-window";
const SEED_N: usize = 1500;

async fn seed_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();

    let run_id = repo_runs::start(&pool).await.unwrap();
    // One observed_at per row, monotonic +1 second so ordering is unambiguous.
    let base: DateTime<Utc> = Utc.with_ymd_and_hms(2026, 5, 21, 0, 0, 0).unwrap();
    for i in 0..SEED_N {
        let event_id = format!("01J{:023}", i); // ULID-like, 26 chars, monotonic
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

fn cursor_at(i: usize) -> Cursor {
    let base: DateTime<Utc> = Utc.with_ymd_and_hms(2026, 5, 21, 0, 0, 0).unwrap();
    Cursor {
        observed_at: base + chrono::Duration::seconds(i as i64),
        event_id: format!("01J{:023}", i),
    }
}

#[tokio::test]
async fn empty_session_returns_empty() {
    let pool = seed_pool().await;
    let rows = repo_observed::list_session_window(&pool, "no-such-session", None, None, 500)
        .await
        .unwrap();
    assert!(rows.is_empty());
}

#[tokio::test]
async fn no_cursor_returns_newest_limit_asc() {
    let pool = seed_pool().await;
    let rows = repo_observed::list_session_window(&pool, SESS, None, None, 500)
        .await
        .unwrap();
    assert_eq!(rows.len(), 500);
    // Newest 500 means rows[1000..1500] from the seed, ordered ASC on the wire.
    assert_eq!(rows.first().unwrap().event_id, format!("01J{:023}", 1000));
    assert_eq!(rows.last().unwrap().event_id, format!("01J{:023}", 1499));
}

#[tokio::test]
async fn before_only_returns_older_window_asc() {
    let pool = seed_pool().await;
    let cur = cursor_at(500);
    let rows = repo_observed::list_session_window(&pool, SESS, Some(&cur), None, 500)
        .await
        .unwrap();
    // Strictly older than row[500]. Newest 500 of those = rows[0..500].
    assert_eq!(rows.len(), 500);
    assert_eq!(rows.first().unwrap().event_id, format!("01J{:023}", 0));
    assert_eq!(rows.last().unwrap().event_id, format!("01J{:023}", 499));
}

#[tokio::test]
async fn after_only_returns_newer_window_asc() {
    let pool = seed_pool().await;
    let cur = cursor_at(500);
    let rows = repo_observed::list_session_window(&pool, SESS, None, Some(&cur), 500)
        .await
        .unwrap();
    // Strictly newer than row[500]. Oldest 500 of those = rows[501..1001].
    assert_eq!(rows.len(), 500);
    assert_eq!(rows.first().unwrap().event_id, format!("01J{:023}", 501));
    assert_eq!(rows.last().unwrap().event_id, format!("01J{:023}", 1000));
}

#[tokio::test]
async fn both_cursors_returns_slice() {
    let pool = seed_pool().await;
    let after_cur = cursor_at(100);
    let before_cur = cursor_at(200);
    let rows = repo_observed::list_session_window(
        &pool,
        SESS,
        Some(&before_cur),
        Some(&after_cur),
        500,
    )
    .await
    .unwrap();
    // Strictly between row[100] and row[200]. rows[101..200].
    assert_eq!(rows.len(), 99);
    assert_eq!(rows.first().unwrap().event_id, format!("01J{:023}", 101));
    assert_eq!(rows.last().unwrap().event_id, format!("01J{:023}", 199));
}

#[tokio::test]
async fn limit_clamps_to_1000() {
    let pool = seed_pool().await;
    let rows = repo_observed::list_session_window(&pool, SESS, None, None, 5000)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1000);
}

#[tokio::test]
async fn limit_min_one() {
    let pool = seed_pool().await;
    let rows = repo_observed::list_session_window(&pool, SESS, None, None, 0)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
}

/// Regression for DEV-S8-10: cross-source event_ids must order by observed_at
/// primary key, not lexicographic event_id (where `metric:...` > `01J...`).
#[tokio::test]
async fn cross_source_event_ids_order_by_observed_at() {
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    let run_id = repo_runs::start(&pool).await.unwrap();
    let base: DateTime<Utc> = Utc.with_ymd_and_hms(2026, 5, 21, 0, 0, 0).unwrap();
    // Three rows, observed_at strictly increasing, event_ids deliberately
    // lexicographically OUT of order:
    //   t=0  : "metric:foo:bar:0"   ('m' = 0x6d)
    //   t=1  : "01J0000000000000000000000A"  ('0' = 0x30)
    //   t=2  : "zzzz_just_for_chaos"
    for (i, eid) in [
        "metric:foo:bar:0",
        "01J0000000000000000000000A",
        "zzzz_just_for_chaos",
    ]
    .iter()
    .enumerate()
    {
        let raw_id = format!("raw_x_{i}");
        repo_raw::insert_dedup(
            &pool,
            &repo_raw::NewRaw {
                raw_event_id: raw_id.clone(),
                ingest_run_id: run_id.clone(),
                source_type: "test".into(),
                source_uri: format!("test://x{i}"),
                source_line_no: i as i64,
                source_byte_offset: 0,
                payload_sha256: format!("sha_x{i}"),
                payload: b"{}".to_vec(),
                parse_error: None,
                captured_at: chrono::Utc::now(),
            },
        )
        .await
        .unwrap();
        let ev = ObservedEvent {
            event_id: eid.to_string(),
            raw_event_id: raw_id,
            schema_version: "0.5.0".into(),
            session_id: "sess-mixed".into(),
            observed_at: base + chrono::Duration::seconds(i as i64),
            actor: Actor::User,
            kind: EventKind::UserMessage,
            parser_version: "test".into(),
            ..Default::default()
        };
        repo_observed::insert(&pool, &ev).await.unwrap();
    }
    let rows = repo_observed::list_session_window(&pool, "sess-mixed", None, None, 500)
        .await
        .unwrap();
    assert_eq!(rows.len(), 3);
    // ASC by observed_at:
    assert_eq!(rows[0].event_id, "metric:foo:bar:0");
    assert_eq!(rows[1].event_id, "01J0000000000000000000000A");
    assert_eq!(rows[2].event_id, "zzzz_just_for_chaos");
}
