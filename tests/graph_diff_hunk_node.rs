//! Slice-10a follow-up — locks that `rebuild_session` materialises a
//! `diff_hunk` graph_node per `diff_hunk` table row and wires a
//! `caused_diff_hunk` edge from the introducing `tool_call` node.
//!
//! Without this linkage the Files lane stays empty in the WebUI even though
//! the ingest path wrote real hunks (the gap surfaced during slice-10a
//! browser smoke).

use serde_json::Value;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::Row;
use std::io::Write;
use tempfile::NamedTempFile;
use wimcc::db::{migrate, repo_diff_hunk};
use wimcc::graph::build;
use wimcc::ingest::store;
use wimcc::live::NoopSink;

const SESSION_ID: &str = "s_gdh_test";
const TOOL_USE_ID: &str = "toolu_gdh_1";
const FILE_PATH: &str = "/tmp/gdh_x.txt";

fn write_synth_transcript() -> NamedTempFile {
    let assistant = serde_json::json!({
        "type": "assistant",
        "sessionId": SESSION_ID,
        "uuid": "u_a_gdh",
        "parentUuid": null,
        "timestamp": "2026-05-22T10:00:00Z",
        "cwd": "/tmp",
        "gitBranch": "main",
        "userType": "external",
        "entrypoint": "cli",
        "version": "2.1.146",
        "message": {
            "role": "assistant",
            "content": [{
                "type": "tool_use",
                "id": TOOL_USE_ID,
                "name": "Edit",
                "input": {
                    "file_path": FILE_PATH,
                    "old_string": "foo",
                    "new_string": "bar",
                    "replace_all": false
                }
            }]
        }
    });
    let tool_result = serde_json::json!({
        "type": "user",
        "sessionId": SESSION_ID,
        "uuid": "u_u_gdh",
        "parentUuid": "u_a_gdh",
        "timestamp": "2026-05-22T10:00:01Z",
        "cwd": "/tmp",
        "gitBranch": "main",
        "userType": "external",
        "entrypoint": "cli",
        "promptId": "p1",
        "message": {
            "role": "user",
            "content": [{
                "tool_use_id": TOOL_USE_ID,
                "type": "tool_result",
                "content": "ok"
            }]
        },
        "toolUseResult": {
            "filePath": FILE_PATH,
            "oldString": "foo",
            "newString": "bar",
            "structuredPatch": [{
                "oldStart": 1,
                "oldLines": 1,
                "newStart": 1,
                "newLines": 1,
                "lines": ["-foo", "+bar"]
            }],
            "userModified": false,
            "replaceAll": false
        }
    });

    let mut f = NamedTempFile::new().unwrap();
    writeln!(f, "{assistant}").unwrap();
    writeln!(f, "{tool_result}").unwrap();
    f
}

async fn fresh_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    pool
}

#[tokio::test]
async fn rebuild_session_emits_diff_hunk_graph_node() {
    let pool = fresh_pool().await;
    let f = write_synth_transcript();
    store::ingest_file(&pool, f.path(), &NoopSink).await.unwrap();
    build::rebuild_session(&pool, SESSION_ID).await.unwrap();

    let hunks = repo_diff_hunk::list_session(&pool, SESSION_ID)
        .await
        .unwrap();
    assert_eq!(hunks.len(), 1, "fixture should yield one hunk");
    let hunk = &hunks[0];

    let row = sqlx::query(
        "SELECT node_kind, merge_keys, payload, source_event_ids
         FROM graph_node WHERE session_id = ? AND node_kind = 'diff_hunk'",
    )
    .bind(SESSION_ID)
    .fetch_optional(&pool)
    .await
    .unwrap()
    .expect("graph_node should contain a diff_hunk row");

    let merge_keys: Value =
        serde_json::from_str(&row.get::<String, _>("merge_keys")).unwrap();
    assert_eq!(
        merge_keys.get("diff_hunk_id").and_then(|v| v.as_str()),
        Some(hunk.diff_hunk_id.as_str()),
        "merge_keys.diff_hunk_id must match the diff_hunk row id"
    );

    let payload: Value =
        serde_json::from_str(&row.get::<String, _>("payload")).unwrap();
    let h = payload
        .get("hunk")
        .expect("payload.hunk should be present for SourcePanel");
    assert_eq!(h.get("file_path").and_then(|v| v.as_str()), Some(FILE_PATH));
    assert_eq!(h.get("change_type").and_then(|v| v.as_str()), Some("modified"));
    assert_eq!(h.get("lines_added").and_then(|v| v.as_i64()), Some(1));
    assert_eq!(h.get("lines_removed").and_then(|v| v.as_i64()), Some(1));
    assert_eq!(
        h.get("introduced_by_tool_use_id").and_then(|v| v.as_str()),
        Some(TOOL_USE_ID),
        "transcript-only attribution must surface on the node payload"
    );
    assert!(
        h.get("introduced_by_commit_sha").is_none(),
        "slice-10a removed git attribution; payload must not carry a commit sha"
    );

    let src_ids: Value =
        serde_json::from_str(&row.get::<String, _>("source_event_ids")).unwrap();
    let src = src_ids.as_array().expect("source_event_ids is an array");
    assert!(
        !src.is_empty(),
        "diff_hunk node must reference the introducing event so SourcePanel can navigate"
    );
}

#[tokio::test]
async fn rebuild_session_emits_caused_diff_hunk_edge() {
    let pool = fresh_pool().await;
    let f = write_synth_transcript();
    store::ingest_file(&pool, f.path(), &NoopSink).await.unwrap();
    build::rebuild_session(&pool, SESSION_ID).await.unwrap();

    let row = sqlx::query(
        "SELECT ge.from_node_id, ge.to_node_id, ge.edge_kind, ge.attributes,
                src.node_kind AS src_kind, dst.node_kind AS dst_kind
         FROM graph_edge ge
         JOIN graph_node src ON src.node_id = ge.from_node_id
         JOIN graph_node dst ON dst.node_id = ge.to_node_id
         WHERE ge.session_id = ? AND ge.edge_kind = 'caused_diff_hunk'",
    )
    .bind(SESSION_ID)
    .fetch_optional(&pool)
    .await
    .unwrap()
    .expect("expected a caused_diff_hunk edge after rebuild");

    assert_eq!(row.get::<String, _>("src_kind"), "tool_call");
    assert_eq!(row.get::<String, _>("dst_kind"), "diff_hunk");
    let attrs: Value =
        serde_json::from_str(&row.get::<String, _>("attributes")).unwrap();
    assert_eq!(
        attrs.get("tool_use_id").and_then(|v| v.as_str()),
        Some(TOOL_USE_ID),
        "edge must carry tool_use_id so reviewers can confirm the link manually"
    );
}
