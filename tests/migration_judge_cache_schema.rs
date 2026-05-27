//! Slice-15 — locks that migration 0009 creates judge_verdict_cache with correct columns.

#[tokio::test]
async fn migration_creates_judge_verdict_cache_table() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let cols: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM pragma_table_info('judge_verdict_cache')",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    for c in [
        "cache_key",
        "category",
        "model_id",
        "prompt_template_version",
        "evidence_hash",
        "verdict_json",
        "created_at",
        "last_hit_at",
    ] {
        assert!(cols.iter().any(|n| n == c), "missing column {c}");
    }
}

#[tokio::test]
async fn judge_cache_has_index_on_category() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let idx: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type='index' AND tbl_name='judge_verdict_cache'",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(
        idx.iter().any(|n| n == "idx_judge_cache_category"),
        "missing index idx_judge_cache_category"
    );
}
