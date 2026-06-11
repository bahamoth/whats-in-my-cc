//! Slice-18 — Schema invariant: raw_event must have redaction_manifest column
//! after migration 0011.

#[tokio::test]
async fn migration_adds_redaction_manifest_column() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let cols: Vec<String> = sqlx::query_scalar("SELECT name FROM pragma_table_info('raw_event')")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert!(
        cols.iter().any(|c| c == "redaction_manifest"),
        "raw_event must have redaction_manifest column; columns: {cols:?}"
    );
}

#[tokio::test]
async fn redaction_manifest_column_is_nullable_text() {
    use sqlx::Row;
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let rows: Vec<sqlx::sqlite::SqliteRow> =
        sqlx::query("SELECT * FROM pragma_table_info('raw_event')")
            .fetch_all(&pool)
            .await
            .unwrap();
    let col = rows
        .iter()
        .find(|r| r.get::<String, _>("name") == "redaction_manifest")
        .expect("redaction_manifest column must exist");
    let col_type: String = col.get("type");
    let notnull: i64 = col.get("notnull");
    assert!(
        col_type.to_uppercase().contains("TEXT"),
        "redaction_manifest must be TEXT type; got: {col_type}"
    );
    assert_eq!(
        notnull, 0,
        "redaction_manifest must be nullable (notnull=0)"
    );
}
