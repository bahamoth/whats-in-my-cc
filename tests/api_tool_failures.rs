//! insight-redesign #3 — GET /v1/sessions/:id/tool-failures returns a class
//! breakdown and a user-visible-only drill list (spec §6.3, Q1/Q2).
use axum_test::TestServer;
use serde_json::Value;
use sqlx::sqlite::SqlitePoolOptions;
use witmcc::api::AppState;
use witmcc::db::migrate;

async fn pool_with_classified_failures() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();

    // (finding_id, subkind, severity)
    for (fid, subkind, sev) in [
        ("find_uv_1", "user_visible", "high"),
        ("find_int_1", "internal_retry", "info"),
        ("find_int_2", "internal_retry", "info"),
        ("find_ben_1", "benign_nonzero_exit", "info"),
    ] {
        sqlx::query(
            "INSERT OR IGNORE INTO finding \
             (finding_id, session_id, category, subkind, severity, confidence, summary, \
              evidence_refs, evidence_projection, provenance, status) \
             VALUES (?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(fid)
        .bind("sess_tf")
        .bind("tool_failure")
        .bind(subkind)
        .bind(sev)
        .bind(1.0_f64)
        .bind("Tool failed.")
        .bind(r#"["ev_001"]"#)
        .bind(format!(r#"{{"category":"tool_failure","failure_class":"{subkind}"}}"#))
        .bind(r#"{"extractor":"tool_failure@v1","layer":"L1","judge":null}"#)
        .bind("active")
        .execute(&pool)
        .await
        .unwrap();
    }
    pool
}

fn build_server(pool: sqlx::SqlitePool) -> TestServer {
    let state = AppState::new_for_tests(pool);
    TestServer::new(witmcc::api::router(state)).unwrap()
}

#[tokio::test]
async fn tool_failures_summary_splits_classes() {
    let server = build_server(pool_with_classified_failures().await);
    let r = server.get("/v1/sessions/sess_tf/tool-failures").await;
    r.assert_status_ok();
    let body: Value = r.json();
    let d = &body["data"];
    assert_eq!(d["user_visible"].as_i64().unwrap(), 1);
    assert_eq!(d["internal_retry"].as_i64().unwrap(), 2);
    assert_eq!(d["benign_nonzero_exit"].as_i64().unwrap(), 1);
    assert_eq!(d["total"].as_i64().unwrap(), 4);
    // The drill list contains only the user_visible finding.
    let list = d["user_visible_findings"].as_array().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["subkind"].as_str().unwrap(), "user_visible");
    assert_eq!(list[0]["severity"].as_str().unwrap(), "high");
}

#[tokio::test]
async fn findings_filter_by_subkind() {
    let server = build_server(pool_with_classified_failures().await);
    let r = server
        .get("/v1/findings?session_id=sess_tf&category=tool_failure&subkind=user_visible&severity=high")
        .await;
    r.assert_status_ok();
    let data = r.json::<Value>()["data"].as_array().unwrap().clone();
    assert_eq!(data.len(), 1);
    assert_eq!(data[0]["subkind"].as_str().unwrap(), "user_visible");
}
