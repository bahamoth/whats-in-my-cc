//! Slice-19 — Schema invariants for migration 0012 (retention_tombstone).
//!
//! These tests will FAIL until migration 0012 is applied.

#[tokio::test]
async fn retention_tombstone_table_has_expected_columns() {
    let pool = wimcc::db::connect(":memory:").await.unwrap();
    wimcc::db::migrate(&pool).await.unwrap();

    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT name FROM pragma_table_info('retention_tombstone') ORDER BY name",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let cols: Vec<&str> = rows.iter().map(|(n,)| n.as_str()).collect();
    assert!(cols.contains(&"resource_id"), "should have resource_id");
    assert!(cols.contains(&"resource_kind"), "should have resource_kind");
    assert!(cols.contains(&"deleted_at"), "should have deleted_at");
    assert!(cols.contains(&"reason"), "should have reason");
}

#[tokio::test]
async fn retention_tombstone_resource_id_is_primary_key() {
    let pool = wimcc::db::connect(":memory:").await.unwrap();
    wimcc::db::migrate(&pool).await.unwrap();

    // Try to insert duplicate — should fail
    sqlx::query(
        "INSERT INTO retention_tombstone (resource_id, resource_kind) VALUES ('r1', 'finding')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let r = sqlx::query(
        "INSERT INTO retention_tombstone (resource_id, resource_kind) VALUES ('r1', 'finding')",
    )
    .execute(&pool)
    .await;
    assert!(r.is_err(), "duplicate resource_id should fail (PRIMARY KEY constraint)");
}
