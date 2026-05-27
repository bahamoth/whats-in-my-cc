#[tokio::test]
async fn migration_creates_episode_table() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let cols: Vec<(String, String)> = sqlx::query_as(
        "SELECT name, type FROM pragma_table_info('episode')",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let names: Vec<&str> = cols.iter().map(|c| c.0.as_str()).collect();
    for c in [
        "episode_id",
        "schema_version",
        "session_id",
        "phase",
        "start_event_id",
        "end_event_id",
        "started_at",
        "ended_at",
        "evidence_node_ids",
        "classification_basis",
        "confidence",
        "summary",
        "classifier_version",
        "created_at",
    ] {
        assert!(names.contains(&c), "missing column {}", c);
    }
}
