use witmcc::db::{migrate, repo_observed};
use witmcc::ingest::store;

#[tokio::test]
async fn backfills_turn_id_for_assistant_in_minimal_session() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1)
        .connect("sqlite::memory:").await.unwrap();
    migrate(&pool).await.unwrap();
    store::ingest_file(&pool, std::path::Path::new("tests/fixtures/transcripts/minimal_session.jsonl")).await.unwrap();
    let evs = repo_observed::list_session(&pool, "sess-A", 100).await.unwrap();
    // The assistant_message with parent u1 should inherit turn_id="p1".
    let assistant = evs.iter().find(|e| e.event_uuid.as_deref() == Some("a1") && matches!(e.kind, witmcc::model::observed::EventKind::AssistantMessage)).unwrap();
    assert_eq!(assistant.turn_id.as_deref(), Some("p1"));
}
