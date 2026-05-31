//! Slice 3 B2 — episode accumulation regression net.
//!
//! `rebuild_session` content-hashes episode ids over
//! `(session_id, phase, start_event_id, end_event_id)`. When a live session
//! GROWS and rebuilds, the trailing ("last open") episode gets a new
//! end_event_id → new id, so `INSERT OR REPLACE` would ADD a row rather than
//! replace the previous trailing episode, leaving a stale row behind. Over a
//! long live session this accumulates (observed: one start event spawning 124
//! episodes, hundreds of zero-duration rows).
//!
//! These tests drive the real `rebuild_session` code path against synthetic
//! events so we can construct a *multi-event trailing episode* whose
//! end_event_id shifts as the session grows — the exact condition the real
//! fixtures (which alternate phase every event → single-event episodes) never
//! exercise.
//!
//! Pre-fix: the grown rebuild leaves the old trailing episode behind (row
//! count grows past the clean single-rebuild count, and a duplicate-prefix span
//! survives). RED.
//! Post-fix: deleting the session's episodes before re-inserting keeps it flat.

use std::collections::HashSet;

use chrono::{TimeZone, Utc};
use sqlx::sqlite::SqlitePoolOptions;
use witmcc::db::{migrate, repo_episode, repo_observed, repo_raw, repo_runs};
use witmcc::graph::build;
use witmcc::model::observed::{Actor, EventKind, ObservedEvent};

const SID: &str = "sess_accum_test";

/// A read-only `Read` tool_call at second `i`. All such events classify as
/// Exploration, so a run of them collapses into ONE trailing episode whose
/// end_event_id is the last event — exactly the span that shifts on growth.
fn read_call(i: i64) -> ObservedEvent {
    ObservedEvent {
        event_id: format!("ev_{i:03}"),
        raw_event_id: format!("raw_{i:03}"),
        schema_version: "observed_event.v1".into(),
        session_id: SID.into(),
        observed_at: Utc.timestamp_opt(1_700_000_000 + i, 0).unwrap(),
        actor: Actor::Assistant,
        kind: EventKind::ToolCall,
        tool_name: Some("Read".into()),
        payload: serde_json::json!({"tool_name": "Read", "input": {}}),
        parser_version: "test".into(),
        ..Default::default()
    }
}

async fn mk_pool() -> (sqlx::SqlitePool, String) {
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    let run_id = repo_runs::start(&pool).await.unwrap();
    (pool, run_id)
}

/// Insert an `ObservedEvent`, first seeding the `raw_event` row it references
/// (observed_event.raw_event_id → raw_event.raw_event_id FK, migration 0001).
async fn insert_event(pool: &sqlx::SqlitePool, run_id: &str, e: &ObservedEvent) {
    repo_raw::insert_dedup(
        pool,
        &repo_raw::NewRaw {
            raw_event_id: e.raw_event_id.clone(),
            ingest_run_id: run_id.to_string(),
            source_type: "test".into(),
            source_uri: format!("test://{}", e.event_id),
            source_line_no: 0,
            source_byte_offset: 0,
            payload_sha256: format!("sha_{}", e.event_id),
            payload: b"{}".to_vec(),
            parse_error: None,
            captured_at: chrono::Utc::now(),
            redaction_state: "not_applicable".into(),
            redaction_manifest: None,
        },
    )
    .await
    .unwrap();
    repo_observed::insert(pool, e).await.unwrap();
}

fn assert_no_duplicate_spans(eps: &[repo_episode::EpisodeRow]) {
    let mut spans: HashSet<(String, String, String)> = HashSet::new();
    for ep in eps {
        let key = (
            ep.session_id.clone(),
            ep.start_event_id.clone(),
            ep.end_event_id.clone(),
        );
        assert!(
            spans.insert(key.clone()),
            "duplicate episode span: {key:?}"
        );
    }
}

#[tokio::test]
async fn rebuild_session_does_not_accumulate_when_trailing_episode_grows() {
    let (pool, run_id) = mk_pool().await;

    // A user_message (Intake) followed by three read-only calls (one trailing
    // Exploration episode spanning ev_001..ev_003).
    let mut events = vec![ObservedEvent {
        event_id: "ev_000".into(),
        raw_event_id: "raw_000".into(),
        schema_version: "observed_event.v1".into(),
        session_id: SID.into(),
        observed_at: Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
        actor: Actor::User,
        kind: EventKind::UserMessage,
        parser_version: "test".into(),
        ..Default::default()
    }];
    for i in 1..=3 {
        events.push(read_call(i));
    }
    for e in &events {
        insert_event(&pool, &run_id, e).await;
    }
    build::rebuild_session(&pool, SID).await.unwrap();
    let before = repo_episode::list_session(&pool, SID).await.unwrap();
    let n_before = before.len();
    assert!(n_before >= 2, "expected Intake + Exploration episodes");

    // Session GROWS: a fourth read-only call arrives. The trailing Exploration
    // episode now ends at ev_004 → new content hash → new id.
    insert_event(&pool, &run_id, &read_call(4)).await;
    build::rebuild_session(&pool, SID).await.unwrap();
    let after = repo_episode::list_session(&pool, SID).await.unwrap();

    // The correct outcome: the SAME number of episodes as before growth
    // (Intake + one Exploration), NOT n_before + a stale trailing row.
    assert_eq!(
        after.len(),
        n_before,
        "growing rebuild accumulated stale episodes (was {n_before}, now {}); \
         the trailing episode's end_event_id shifted so its old row was not replaced",
        after.len()
    );
    assert_no_duplicate_spans(&after);

    // The stale ev_001..ev_003 Exploration span must NOT survive.
    assert!(
        !after.iter().any(|e| e.start_event_id == "ev_001" && e.end_event_id == "ev_003"),
        "stale trailing span ev_001..ev_003 survived growth to ev_004"
    );
}

/// Idempotency: a plain double-rebuild over an unchanged session stays flat.
#[tokio::test]
async fn rebuild_session_double_rebuild_is_idempotent() {
    let (pool, run_id) = mk_pool().await;
    let events = [
        ObservedEvent {
            event_id: "ev_000".into(),
            raw_event_id: "raw_000".into(),
            schema_version: "observed_event.v1".into(),
            session_id: SID.into(),
            observed_at: Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
            actor: Actor::User,
            kind: EventKind::UserMessage,
            parser_version: "test".into(),
            ..Default::default()
        },
        read_call(1),
        read_call(2),
    ];
    for e in &events {
        insert_event(&pool, &run_id, e).await;
    }

    build::rebuild_session(&pool, SID).await.unwrap();
    let n = repo_episode::list_session(&pool, SID).await.unwrap().len();
    assert!(n > 0);

    build::rebuild_session(&pool, SID).await.unwrap();
    let again = repo_episode::list_session(&pool, SID).await.unwrap();
    assert_eq!(again.len(), n, "double rebuild must not change row count");
    assert_no_duplicate_spans(&again);
}
