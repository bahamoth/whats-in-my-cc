//! Slice-10a — locks the transcript-driven diff_hunk write path. Ingests a
//! fixture transcript whose lines are real `tool_result`s with non-empty
//! `structuredPatch`, then asserts the diff_hunk table is populated with
//! one row per fixture hunk, scoped to the real session_id (never to a
//! synthetic value like `"filesystem"`).
//!
//! Two-pass also verifies idempotency: re-ingesting the same file does not
//! duplicate hunks.

use sqlx::sqlite::SqlitePoolOptions;
use sqlx::Row;
use std::io::Write;
use tempfile::NamedTempFile;
use witmcc::db::{migrate, repo_diff_hunk};
use witmcc::ingest::store;
use witmcc::live::NoopSink;

/// Build a synthetic minimal transcript JSONL with one assistant Edit
/// tool_use + one user tool_result containing a fixture-shaped structuredPatch.
/// Avoids depending on the 3-line frozen fixture which uses real cwd paths
/// and could surprise readers.
fn write_synth_transcript() -> NamedTempFile {
    let session_id = "s_ingest_test";
    let tool_use_id = "toolu_test1";
    let assistant_uuid = "u_assistant_1";
    let user_uuid = "u_user_1";

    let assistant = serde_json::json!({
        "type": "assistant",
        "sessionId": session_id,
        "uuid": assistant_uuid,
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
                "id": tool_use_id,
                "name": "Edit",
                "input": {
                    "file_path": "/tmp/x.txt",
                    "old_string": "foo",
                    "new_string": "bar",
                    "replace_all": false
                }
            }]
        }
    });
    let tool_result = serde_json::json!({
        "type": "user",
        "sessionId": session_id,
        "uuid": user_uuid,
        "parentUuid": assistant_uuid,
        "timestamp": "2026-05-22T10:00:01Z",
        "cwd": "/tmp",
        "gitBranch": "main",
        "userType": "external",
        "entrypoint": "cli",
        "promptId": "p1",
        "message": {
            "role": "user",
            "content": [{
                "tool_use_id": tool_use_id,
                "type": "tool_result",
                "content": "ok"
            }]
        },
        "toolUseResult": {
            "filePath": "/tmp/x.txt",
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
    writeln!(f, "{}", assistant).unwrap();
    writeln!(f, "{}", tool_result).unwrap();
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
async fn ingest_writes_diff_hunk_row_per_structured_patch_hunk() {
    let pool = fresh_pool().await;
    let tmp = write_synth_transcript();
    store::ingest_file(&pool, tmp.path(), &NoopSink).await.unwrap();

    let rows = repo_diff_hunk::list_session(&pool, "s_ingest_test")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1, "expected 1 hunk; got rows: {rows:#?}");
    let r = &rows[0];
    assert_eq!(r.session_id, "s_ingest_test");
    assert_eq!(r.file_path, "/tmp/x.txt");
    assert_eq!(r.change_type, "modified");
    assert_eq!(r.lines_added, 1);
    assert_eq!(r.lines_removed, 1);
    assert!(!r.introduced_by_event_id.is_empty());
    assert_eq!(r.introduced_by_tool_use_id.as_deref(), Some("toolu_test1"));
    assert!(!r.user_modified);
}

#[tokio::test]
async fn no_diff_hunk_uses_synthetic_filesystem_session() {
    // Negative invariant: after slice-10a no row ever has session_id =
    // "filesystem". Ingest a transcript; query directly.
    let pool = fresh_pool().await;
    let tmp = write_synth_transcript();
    store::ingest_file(&pool, tmp.path(), &NoopSink).await.unwrap();
    let row = sqlx::query("SELECT COUNT(*) AS c FROM diff_hunk WHERE session_id = 'filesystem'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row.get::<i64, _>("c"), 0);
}

#[tokio::test]
async fn reingest_is_idempotent_on_diff_hunk() {
    let pool = fresh_pool().await;
    let tmp = write_synth_transcript();
    store::ingest_file(&pool, tmp.path(), &NoopSink).await.unwrap();
    let before = repo_diff_hunk::count_by_session(&pool, "s_ingest_test")
        .await
        .unwrap();
    // Re-ingest the same file — raw rows dedupe by (source_uri, line_no,
    // payload_sha), so observed/diff_hunk should also stay stable.
    store::ingest_file(&pool, tmp.path(), &NoopSink).await.unwrap();
    let after = repo_diff_hunk::count_by_session(&pool, "s_ingest_test")
        .await
        .unwrap();
    assert_eq!(before, after, "diff_hunk row count must stay stable on re-ingest");
}

#[tokio::test]
async fn write_tool_result_produces_no_hunks() {
    // Write's structuredPatch is always [] — extractor must yield zero
    // rows and the ingest path must not crash.
    let session_id = "s_write_only";
    let tool_use_id = "toolu_write_1";
    let assistant = serde_json::json!({
        "type": "assistant",
        "sessionId": session_id,
        "uuid": "u_a_w",
        "parentUuid": null,
        "timestamp": "2026-05-22T10:00:00Z",
        "message": { "role": "assistant", "content": [{
            "type": "tool_use", "id": tool_use_id, "name": "Write",
            "input": { "file_path": "/tmp/n.txt", "content": "hi" }
        }]}
    });
    let result = serde_json::json!({
        "type": "user",
        "sessionId": session_id,
        "uuid": "u_u_w",
        "parentUuid": "u_a_w",
        "timestamp": "2026-05-22T10:00:01Z",
        "message": { "role": "user", "content": [{
            "tool_use_id": tool_use_id, "type": "tool_result", "content": "wrote"
        }]},
        "toolUseResult": {
            "filePath": "/tmp/n.txt",
            "oldString": null,
            "newString": null,
            "structuredPatch": [],
            "userModified": false
        }
    });
    let mut f = NamedTempFile::new().unwrap();
    writeln!(f, "{}", assistant).unwrap();
    writeln!(f, "{}", result).unwrap();
    let pool = fresh_pool().await;
    store::ingest_file(&pool, f.path(), &NoopSink).await.unwrap();
    let n = repo_diff_hunk::count_by_session(&pool, session_id).await.unwrap();
    assert_eq!(n, 0, "Write must produce zero hunks (structuredPatch=[])");
}
