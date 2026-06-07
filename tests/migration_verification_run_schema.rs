//! Slice-11 — migration invariant test: verifies that the
//! `0005_verification_run` migration creates the expected table + columns.
//! (TDD red — Phase 1 commit 1.)

use sqlx::SqlitePool;

#[tokio::test]
async fn migration_creates_verification_run_table_with_expected_columns() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();

    let cols: Vec<(String, String, i64, i64)> = sqlx::query_as(
        "SELECT name, type, \"notnull\", pk FROM pragma_table_info('verification_run')",
    )
    .fetch_all(&pool)
    .await
    .expect("verification_run table must exist after migration");

    assert!(
        !cols.is_empty(),
        "verification_run table must exist and have columns"
    );

    let names: Vec<String> = cols.iter().map(|c| c.0.clone()).collect();

    let expected_cols = &[
        "verification_run_id",
        "schema_version",
        "session_id",
        "source",
        "command",
        "command_kind",
        "trigger_event_id",
        "trigger_tool_use_id",
        "status",
        "status_provenance",
        "started_at",
        "ended_at",
        "exit_code",
        "failure_summary",
        "raw_event_id",
        "parser_version",
        "created_at",
        "detection_basis",
        "status_basis",
    ];
    for c in expected_cols {
        assert!(
            names.contains(&c.to_string()),
            "missing column '{c}'; present columns: {names:?}"
        );
    }

    // verification_run_id is the PRIMARY KEY (pk == 1)
    let pk_col = cols
        .iter()
        .find(|c| c.0 == "verification_run_id")
        .expect("verification_run_id column must exist");
    assert_eq!(
        pk_col.3, 1,
        "verification_run_id must be the primary key (pk=1)"
    );

    // status, session_id, source, command, trigger_event_id must be NOT NULL
    for notnull_col in &["session_id", "source", "command", "command_kind",
                          "trigger_event_id", "status", "started_at",
                          "raw_event_id", "parser_version",
                          "detection_basis", "status_basis"] {
        let col = cols.iter().find(|c| c.0 == *notnull_col)
            .unwrap_or_else(|| panic!("column {notnull_col} must exist"));
        assert_eq!(col.2, 1, "column '{notnull_col}' must be NOT NULL");
    }
}

#[tokio::test]
async fn migration_creates_expected_indexes_on_verification_run() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();

    let indexes: Vec<(String,)> = sqlx::query_as(
        "SELECT name FROM sqlite_master WHERE type='index' AND tbl_name='verification_run'",
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    let index_names: Vec<&str> = indexes.iter().map(|(n,)| n.as_str()).collect();
    assert!(
        index_names.contains(&"idx_verification_run_session_started"),
        "missing session+started index; found: {index_names:?}"
    );
    assert!(
        index_names.contains(&"idx_verification_run_trigger"),
        "missing trigger_event_id index; found: {index_names:?}"
    );
}
