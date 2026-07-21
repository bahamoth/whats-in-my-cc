use chrono::Utc;
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use tempfile::TempDir;
use wimcc::db::{migrate, repo_raw, repo_runs};

async fn test_pool() -> (SqlitePool, TempDir) {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("test.db");
    let url = format!("sqlite://{}?mode=rwc", db_path.display());
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    (pool, tmp)
}

#[tokio::test]
async fn idempotent_insert() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    let run_id = repo_runs::start(&pool).await.unwrap();
    let row = repo_raw::NewRaw {
        raw_event_id: "01HXAAA".into(),
        ingest_run_id: run_id.clone(),
        source_type: "claude_transcript".into(),
        source_uri: "/tmp/x.jsonl".into(),
        source_line_no: 1,
        source_byte_offset: 0,
        payload_sha256: "deadbeef".into(),
        payload: b"hello".to_vec(),
        parse_error: None,
        captured_at: Utc::now(),
        redaction_state: "not_applicable".into(),
        redaction_manifest: None,
    };
    let inserted_first = repo_raw::insert_dedup(&pool, &row).await.unwrap();
    let inserted_second = repo_raw::insert_dedup(&pool, &row).await.unwrap();
    assert!(inserted_first, "first insert should report newly inserted");
    assert!(
        !inserted_second,
        "second insert with identical (uri,line,sha) is a no-op"
    );
    let count: (i64,) = sqlx::query_as("SELECT count(*) FROM raw_event")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count.0, 1);
}

#[tokio::test]
async fn get_for_event_id_returns_joined_row() {
    let (pool, _tmp) = test_pool().await;
    // seed minimal ingest_run + raw_event + observed_event
    let run_id = "run_test_x";
    sqlx::query(
        "INSERT INTO ingest_run(run_id, started_at, status) \
         VALUES (?, ?, 'ok')",
    )
    .bind(run_id)
    .bind("2026-05-19T00:00:00Z")
    .execute(&pool)
    .await
    .unwrap();

    let raw = wimcc::db::repo_raw::NewRaw {
        raw_event_id: "raw_x_001".into(),
        ingest_run_id: run_id.into(),
        source_type: "transcript".into(),
        source_uri: "/tmp/sample.jsonl".into(),
        source_line_no: 7,
        source_byte_offset: 0,
        payload_sha256: "deadbeef".into(),
        payload: br#"{"type":"user","content":"hi"}"#.to_vec(),
        parse_error: None,
        captured_at: chrono::Utc::now(),
        redaction_state: "not_applicable".into(),
        redaction_manifest: None,
    };
    wimcc::db::repo_raw::insert_dedup(&pool, &raw)
        .await
        .unwrap();

    // synthesize an observed_event referencing the raw row
    sqlx::query(
        "INSERT INTO observed_event(\
            event_id, raw_event_id, schema_version, session_id, event_uuid, \
            observed_at, actor, kind, is_sidechain, is_meta, payload, parser_version)\
         VALUES ('ev_x_001','raw_x_001','1.0','sess_x','uuid-1',\
                 '2026-05-19T00:00:00Z','user','user_message',0,0,'{}','0.1')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let row = wimcc::db::repo_raw::get_for_event_id(&pool, "ev_x_001")
        .await
        .unwrap()
        .expect("row");
    assert_eq!(row.event_id, "ev_x_001");
    assert_eq!(row.session_id, "sess_x");
    assert_eq!(row.source_uri, "/tmp/sample.jsonl");
    assert_eq!(row.source_line_no, 7);
    assert_eq!(row.source_type, "transcript");
    assert_eq!(row.kind, "user_message");
    assert!(row.payload.starts_with(b"{"));
}

#[tokio::test]
async fn get_for_event_id_returns_none_when_missing() {
    let (pool, _tmp) = test_pool().await;
    let row = wimcc::db::repo_raw::get_for_event_id(&pool, "no_such_event")
        .await
        .unwrap();
    assert!(row.is_none());
}
