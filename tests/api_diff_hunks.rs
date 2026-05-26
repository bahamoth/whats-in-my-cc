//! Slice-10a follow-up — `/v1/sessions/{id}/diff-hunks` HTTP read endpoint.
//! Exposes the side-table the slice-10a wire-up populates so reviewers can
//! confirm transcript-derived attribution without dropping into SQLite.

use axum_test::TestServer;
use serde_json::Value;
use sqlx::sqlite::SqlitePoolOptions;
use std::io::Write;
use tempfile::NamedTempFile;
use witmcc::db::migrate;
use witmcc::ingest::store;
use witmcc::live::NoopSink;

const SESSION_ID: &str = "s_api_dh_test";
const TOOL_USE_ID: &str = "toolu_api_dh";
const FILE_PATH: &str = "/tmp/api_dh.txt";

fn write_synth_transcript() -> NamedTempFile {
    let assistant = serde_json::json!({
        "type": "assistant",
        "sessionId": SESSION_ID,
        "uuid": "u_a_api",
        "parentUuid": null,
        "timestamp": "2026-05-22T11:00:00Z",
        "cwd": "/tmp",
        "userType": "external",
        "entrypoint": "cli",
        "version": "2.1.146",
        "message": {
            "role": "assistant",
            "content": [{
                "type": "tool_use",
                "id": TOOL_USE_ID,
                "name": "Edit",
                "input": {"file_path": FILE_PATH, "old_string": "a", "new_string": "b"}
            }]
        }
    });
    let tool_result = serde_json::json!({
        "type": "user",
        "sessionId": SESSION_ID,
        "uuid": "u_u_api",
        "parentUuid": "u_a_api",
        "timestamp": "2026-05-22T11:00:01Z",
        "cwd": "/tmp",
        "userType": "external",
        "entrypoint": "cli",
        "message": {"role": "user", "content": [{
            "tool_use_id": TOOL_USE_ID, "type": "tool_result", "content": "ok"
        }]},
        "toolUseResult": {
            "filePath": FILE_PATH,
            "oldString": "a",
            "newString": "b",
            "structuredPatch": [{
                "oldStart": 10, "oldLines": 1, "newStart": 10, "newLines": 1,
                "lines": ["-a", "+b"]
            }],
            "userModified": false
        }
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
    let f = write_synth_transcript();
    store::ingest_file(&pool, f.path(), &NoopSink).await.unwrap();
    let app = witmcc::api::router(witmcc::api::AppState::new_for_tests(pool));
    TestServer::new(app).unwrap()
}

#[tokio::test]
async fn list_diff_hunks_returns_attributed_rows() {
    let s = setup().await;
    let resp = s
        .get(&format!("/v1/sessions/{SESSION_ID}/diff-hunks"))
        .await;
    resp.assert_status_ok();
    let v: Value = resp.json();

    assert_eq!(v["meta"]["schema_version"], "0.5.0");
    let hunks = v["data"]["hunks"]
        .as_array()
        .expect("data.hunks must be an array");
    assert_eq!(hunks.len(), 1, "synth transcript should yield one hunk");
    let h = &hunks[0];
    assert_eq!(h["file_path"], FILE_PATH);
    assert_eq!(h["change_type"], "modified");
    assert_eq!(h["lines_added"], 1);
    assert_eq!(h["lines_removed"], 1);
    assert_eq!(h["introduced_by_tool_use_id"], TOOL_USE_ID);
    assert_eq!(h["user_modified"], false);
    assert!(
        h["introduced_by_event_id"].as_str().is_some(),
        "transcript event attribution must surface verbatim"
    );
    assert!(
        h.get("introduced_by_commit_sha").is_none(),
        "slice-10a removed git attribution — response must not leak a sha field"
    );
    // patch_preview is opaque to the client but must be present.
    assert!(h["patch_preview"].is_string());
}

#[tokio::test]
async fn unknown_session_returns_empty_hunks() {
    // Listing an unknown session must NOT 404 — a session with zero edits is
    // legitimate (OTel-only sessions are common). Same shape, empty array.
    let s = setup().await;
    let resp = s.get("/v1/sessions/does-not-exist/diff-hunks").await;
    resp.assert_status_ok();
    let v: Value = resp.json();
    let hunks = v["data"]["hunks"].as_array().unwrap();
    assert!(hunks.is_empty());
}
