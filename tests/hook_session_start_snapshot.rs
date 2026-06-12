//! SessionStart hook 수신 시각의 CLAUDE.md 스냅샷 capture (Task 2).
//!
//! Real-data anchoring: cwd 없는 동결 fixture(tests/fixtures/hook/session_start.json)
//! 는 capture가 발생하지 않는 degrade 케이스를 잠근다. cwd 있는 케이스는
//! 공식 docs(https://code.claude.com/docs/en/hooks)의 공통 stdin 필드(cwd)를 따른다.

use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::sqlite::SqlitePoolOptions;
use wimcc::db::{migrate, repo_observed};
use wimcc::ingest::hook::{parse_body, store_with_env, SnapshotEnv};
use wimcc::live::NoopSink;

async fn test_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    pool
}

fn scratch(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir()
        .join("wimcc_hook_snap")
        .join(format!("{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[tokio::test]
async fn session_start_with_cwd_captures_claude_md_hashes() {
    let pool = test_pool().await;
    let root = scratch("capture");
    let cwd = root.join("proj");
    std::fs::create_dir_all(&cwd).unwrap();
    std::fs::write(cwd.join("CLAUDE.md"), "rules v1").unwrap();
    let home = root.join("home");
    std::fs::create_dir_all(home.join(".claude")).unwrap();
    std::fs::write(home.join(".claude").join("CLAUDE.md"), "user rules").unwrap();

    let body = serde_json::json!({
        "session_id": "sess_snap_1",
        "hook_event_name": "SessionStart",
        "source": "startup",
        "cwd": cwd.to_string_lossy(),
    });
    let env = SnapshotEnv {
        home: Some(home.clone()),
    };
    store_with_env(
        &pool,
        parse_body(&body),
        chrono::Utc::now(),
        &NoopSink,
        &env,
    )
    .await
    .unwrap();

    let rows = repo_observed::list_session(&pool, "sess_snap_1", 10)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    let captured = rows[0]
        .payload
        .pointer("/captured/claude_md")
        .expect("session_start payload must carry /captured/claude_md")
        .as_array()
        .unwrap()
        .clone();
    let proj_path = cwd.join("CLAUDE.md").to_string_lossy().into_owned();
    let user_path = home
        .join(".claude")
        .join("CLAUDE.md")
        .to_string_lossy()
        .into_owned();
    let by_path = |p: &str| -> Value {
        captured
            .iter()
            .find(|e| e["path"].as_str() == Some(p))
            .unwrap_or_else(|| panic!("missing snapshot entry {p}"))
            .clone()
    };
    assert_eq!(
        by_path(&proj_path)["sha256"].as_str().unwrap(),
        hex::encode(Sha256::digest(b"rules v1"))
    );
    assert_eq!(
        by_path(&user_path)["bytes"].as_u64().unwrap(),
        "user rules".len() as u64
    );
    assert!(rows[0].payload.pointer("/captured/captured_at").is_some());
    // raw(hook 원문)는 그대로 보존 — capture는 observed payload에만 붙는다.
    assert!(rows[0].payload.pointer("/hook/session_id").is_some());
}

#[tokio::test]
async fn session_start_without_cwd_degrades_to_no_capture() {
    let pool = test_pool().await;
    // 동결 실 fixture: cwd 없는 최소형 (sess_fix_A).
    let body: Value = serde_json::from_str(
        &std::fs::read_to_string("tests/fixtures/hook/session_start.json").unwrap(),
    )
    .unwrap();
    let env = SnapshotEnv { home: None };
    store_with_env(
        &pool,
        parse_body(&body),
        chrono::Utc::now(),
        &NoopSink,
        &env,
    )
    .await
    .unwrap();
    let rows = repo_observed::list_session(&pool, "sess_fix_A", 10)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert!(
        rows[0].payload.pointer("/captured").is_none(),
        "no cwd → no capture key at all"
    );
}

#[tokio::test]
async fn non_session_start_hooks_never_capture() {
    let pool = test_pool().await;
    // 동결 실 fixture pre_tool_use.json (sess_fix_A) — cwd가 있어도
    // session_start가 아니면 capture하지 않는다.
    let body: Value = serde_json::from_str(
        &std::fs::read_to_string("tests/fixtures/hook/pre_tool_use.json").unwrap(),
    )
    .unwrap();
    let env = SnapshotEnv { home: None };
    store_with_env(
        &pool,
        parse_body(&body),
        chrono::Utc::now(),
        &NoopSink,
        &env,
    )
    .await
    .unwrap();
    let rows = repo_observed::list_session(&pool, "sess_fix_A", 10)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].payload.pointer("/captured").is_none());
}
