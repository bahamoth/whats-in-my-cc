//! Slice-14 — locks that migration 0008 creates the `finding` table with the
//! correct column set and that `status` defaults to `"active"`.

#[tokio::test]
async fn migration_creates_finding_table() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let cols: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM pragma_table_info('finding')",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    for c in [
        "finding_id",
        "schema_version",
        "session_id",
        "category",
        "severity",
        "confidence",
        "summary",
        "evidence_refs",
        "evidence_projection",
        "provenance",
        "status",
        "created_at",
    ] {
        assert!(cols.iter().any(|n| n == c), "missing column {c}");
    }
}

#[tokio::test]
async fn finding_default_status_is_active() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO finding \
         (finding_id, session_id, category, severity, confidence, summary, \
          evidence_refs, evidence_projection, provenance) \
         VALUES (?,?,?,?,?,?,?,?,?)",
    )
    .bind("find_x")
    .bind("sess_x")
    .bind("missing_verification")
    .bind("medium")
    .bind(0.9_f64)
    .bind("test summary")
    .bind("[]")
    .bind("{}")
    .bind("{}")
    .execute(&pool)
    .await
    .unwrap();

    let status: String =
        sqlx::query_scalar("SELECT status FROM finding WHERE finding_id='find_x'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "active");
}

#[tokio::test]
async fn finding_table_has_subkind_column() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let cols: Vec<String> = sqlx::query_scalar("SELECT name FROM pragma_table_info('finding')")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert!(
        cols.iter().any(|c| c == "subkind"),
        "finding table must have a subkind column; got {cols:?}"
    );
}
