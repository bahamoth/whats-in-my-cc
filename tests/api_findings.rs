//! Slice-11 — `/v1/sessions/{id}/findings` HTTP read endpoint.

use axum_test::TestServer;
use serde_json::Value;
use sqlx::sqlite::SqlitePoolOptions;
use std::io::Write;
use tempfile::NamedTempFile;
use witmcc::db::migrate;
use witmcc::ingest::store;
use witmcc::live::NoopSink;

const SESSION_ID: &str = "s_api_find";
const TOOL_USE_ID: &str = "toolu_api_find";

fn write_failing_tool_transcript() -> NamedTempFile {
    let assistant = serde_json::json!({
        "type": "assistant",
        "sessionId": SESSION_ID,
        "uuid": "u_a_find",
        "parentUuid": null,
        "timestamp": "2026-05-26T10:00:00Z",
        "cwd": "/tmp",
        "userType": "external",
        "entrypoint": "cli",
        "version": "2.1.146",
        "message": {
            "role": "assistant",
            "content": [{
                "type": "tool_use",
                "id": TOOL_USE_ID,
                "name": "Bash",
                "input": {"command": "ls /nope"}
            }]
        }
    });
    let tool_result = serde_json::json!({
        "type": "user",
        "sessionId": SESSION_ID,
        "uuid": "u_u_find",
        "parentUuid": "u_a_find",
        "timestamp": "2026-05-26T10:00:01Z",
        "cwd": "/tmp",
        "userType": "external",
        "entrypoint": "cli",
        "message": {"role": "user", "content": [{
            "tool_use_id": TOOL_USE_ID,
            "type": "tool_result",
            "content": "ls: /nope: No such file or directory",
            "is_error": true
        }]}
    });
    let mut f = NamedTempFile::new().unwrap();
    writeln!(f, "{assistant}").unwrap();
    writeln!(f, "{tool_result}").unwrap();
    f
}

async fn setup() -> TestServer {
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    let f = write_failing_tool_transcript();
    // Slice-11 — ingest_file invokes rebuild_session, which auto-runs insight
    // rules in the same transaction. No explicit insight call needed here.
    store::ingest_file(&pool, f.path(), &NoopSink).await.unwrap();
    let app = witmcc::api::router(witmcc::api::AppState::new_for_tests(pool));
    TestServer::new(app).unwrap()
}

#[tokio::test]
async fn list_findings_returns_tool_failure() {
    let s = setup().await;
    let resp = s
        .get(&format!("/v1/sessions/{SESSION_ID}/findings"))
        .await;
    resp.assert_status_ok();
    let v: Value = resp.json();

    assert_eq!(v["meta"]["schema_version"], "0.5.0");
    let findings = v["data"]["findings"]
        .as_array()
        .expect("data.findings must be an array");
    assert_eq!(findings.len(), 1);

    let f = &findings[0];
    assert_eq!(f["category"], "tool_failure");
    assert_eq!(f["severity"], "medium");
    assert_eq!(f["rule_version"], "tool_failure.v1");
    assert_eq!(f["schema_version"], "finding.v1");
    assert!(f["confidence"].as_f64().unwrap() >= 0.9);
    assert!(f["claim"].as_str().unwrap().to_lowercase().contains("tool"));
    assert!(f["finding_id"].as_str().unwrap().starts_with("find_"));
    let er = f["evidence_refs"].as_array().expect("evidence_refs array");
    assert_eq!(er.len(), 1);
    assert!(er[0]["node_id"].as_str().unwrap().starts_with("nd_"));
    assert_eq!(er[0]["role"], "supporting");
}

#[tokio::test]
async fn unknown_session_returns_empty_findings() {
    let s = setup().await;
    let resp = s.get("/v1/sessions/does-not-exist/findings").await;
    resp.assert_status_ok();
    let v: Value = resp.json();
    let findings = v["data"]["findings"].as_array().unwrap();
    assert!(findings.is_empty());
}
