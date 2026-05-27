#[tokio::test]
async fn migration_adds_inference_columns_to_graph_edge() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let cols: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('graph_edge')")
            .fetch_all(&pool)
            .await
            .unwrap();
    for c in ["inference_rule_id", "confidence"] {
        assert!(cols.iter().any(|n| n == c), "missing column {c}");
    }
}
