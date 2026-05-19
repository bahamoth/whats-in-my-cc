use sqlx::sqlite::SqlitePoolOptions;

#[tokio::test]
async fn migrate_creates_expected_tables() {
    let url = "sqlite::memory:";
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(url).await.unwrap();
    witmcc::db::migrate(&pool).await.unwrap();
    for t in ["ingest_run","raw_event","observed_event","graph_node","graph_edge"] {
        let row: (i64,) = sqlx::query_as(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?"
        ).bind(t).fetch_one(&pool).await.unwrap();
        assert_eq!(row.0, 1, "missing table: {t}");
    }
}
