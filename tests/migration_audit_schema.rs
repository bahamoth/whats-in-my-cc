//! Slice-19 — Schema invariants for migration 0013 (audit).
//!
//! These tests will FAIL until migration 0013 is applied.

#[tokio::test]
async fn audit_table_has_expected_columns() {
    let pool = witmcc::db::connect(":memory:").await.unwrap();
    witmcc::db::migrate(&pool).await.unwrap();

    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT name FROM pragma_table_info('audit') ORDER BY name",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let cols: Vec<&str> = rows.iter().map(|(n,)| n.as_str()).collect();
    assert!(cols.contains(&"audit_id"), "should have audit_id");
    assert!(cols.contains(&"event"), "should have event");
    assert!(cols.contains(&"actor"), "should have actor");
    assert!(cols.contains(&"payload"), "should have payload");
    assert!(cols.contains(&"created_at"), "should have created_at");
}

#[tokio::test]
async fn audit_insert_and_query_works() {
    let pool = witmcc::db::connect(":memory:").await.unwrap();
    witmcc::db::migrate(&pool).await.unwrap();

    sqlx::query(
        "INSERT INTO audit (audit_id, event, actor, payload) VALUES ('aud_001', 'test.event', 'test_actor', '{\"k\":1}')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let (event,): (String,) =
        sqlx::query_as("SELECT event FROM audit WHERE audit_id = 'aud_001'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(event, "test.event");
}
