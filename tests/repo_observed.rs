use witmcc::db::{migrate, repo_observed, repo_raw, repo_runs};
use witmcc::model::meta::{PARSER_VERSION_TRANSCRIPT, SCHEMA_VERSION};
use witmcc::model::observed::{Actor, EventKind, ObservedEvent};

use sqlx::sqlite::SqlitePoolOptions;

#[tokio::test]
async fn insert_and_list_session_events() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    let run_id = repo_runs::start(&pool).await.unwrap();
    let raw = repo_raw::NewRaw {
        raw_event_id: "raw1".into(),
        ingest_run_id: run_id,
        source_type: "claude_transcript".into(),
        source_uri: "/tmp/x.jsonl".into(),
        source_line_no: 1,
        source_byte_offset: 0,
        payload_sha256: "abc".into(),
        payload: b"{}".to_vec(),
        parse_error: None,
        captured_at: chrono::Utc::now(),
    };
    repo_raw::insert_dedup(&pool, &raw).await.unwrap();

    let e = ObservedEvent {
        event_id: "ev1".into(),
        raw_event_id: "raw1".into(),
        schema_version: SCHEMA_VERSION.into(),
        parser_version: PARSER_VERSION_TRANSCRIPT.into(),
        session_id: "sess".into(),
        observed_at: chrono::Utc::now(),
        actor: Actor::User,
        kind: EventKind::UserMessage,
        payload: serde_json::json!({"x": 1}),
        ..Default::default()
    };
    repo_observed::insert(&pool, &e).await.unwrap();

    let evs = repo_observed::list_session(&pool, "sess", 100)
        .await
        .unwrap();
    assert_eq!(evs.len(), 1);
    assert_eq!(evs[0].event_id, "ev1");
}
