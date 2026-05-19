use sqlx::sqlite::SqlitePoolOptions;
use witmcc::db::{migrate, repo_raw, repo_runs};
use chrono::Utc;

#[tokio::test]
async fn idempotent_insert() {
    let pool = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
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
    };
    let inserted_first = repo_raw::insert_dedup(&pool, &row).await.unwrap();
    let inserted_second = repo_raw::insert_dedup(&pool, &row).await.unwrap();
    assert!(inserted_first, "first insert should report newly inserted");
    assert!(!inserted_second, "second insert with identical (uri,line,sha) is a no-op");
    let count: (i64,) = sqlx::query_as("SELECT count(*) FROM raw_event")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(count.0, 1);
}
