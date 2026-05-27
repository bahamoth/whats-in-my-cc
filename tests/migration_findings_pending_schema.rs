//! Slice-15 — locks that migration 0010 creates findings_pending_judge with correct columns.

#[tokio::test]
async fn migration_creates_findings_pending_judge_table() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let cols: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM pragma_table_info('findings_pending_judge')",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    for c in [
        "candidate_id",
        "schema_version",
        "session_id",
        "category",
        "confidence_l1",
        "evidence_refs",
        "evidence_projection",
        "queued_at",
        "last_attempt_at",
        "attempts",
    ] {
        assert!(cols.iter().any(|n| n == c), "missing column {c}");
    }
}

#[tokio::test]
async fn pending_default_schema_version() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO findings_pending_judge \
         (candidate_id, session_id, category, confidence_l1, evidence_refs, evidence_projection) \
         VALUES (?,?,?,?,?,?)",
    )
    .bind("cand_x")
    .bind("sess_x")
    .bind("noop_test")
    .bind(0.5_f64)
    .bind("[]")
    .bind("{}")
    .execute(&pool)
    .await
    .unwrap();

    let sv: String = sqlx::query_scalar(
        "SELECT schema_version FROM findings_pending_judge WHERE candidate_id='cand_x'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(sv, "pending_finding.v1");
}
