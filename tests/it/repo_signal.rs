use sqlx::sqlite::SqlitePoolOptions;
use wimcc::db::repo_signal::SignalRow;
use wimcc::db::{migrate, repo_signal};

#[tokio::test]
async fn insert_and_list_by_session() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();

    let row = SignalRow {
        signal_id: "sig_abc".into(),
        schema_version: "signal.v1".into(),
        session_id: "sess_1".into(),
        detector: "tool_failure".into(),
        subkind: None,
        summary: "Tool Bash returned is_error=true".into(),
        evidence_refs: "[\"ev_1\"]".into(),
        facts: "{\"is_error\":true}".into(),
        provenance: "{\"detector\":\"tool_failure@v1\"}".into(),
        created_at: "2026-06-07T00:00:00Z".into(),
    };
    repo_signal::insert(&pool, &row).await.unwrap();
    let rows = repo_signal::list_by_session(&pool, "sess_1").await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].detector, "tool_failure");
}
