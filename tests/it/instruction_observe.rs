//! instruction 전향 관측 (스펙 §2 4차 개정) — TDD.
//! 결정론 계약:
//! - 세션 cwd 루트 CLAUDE.md('project')와 ~/.claude/CLAUDE.md('user')를 관측
//!   (user 홈은 테스트에서 주입 가능해야 하므로 home 오버라이드 인자).
//! - 내용 주소화: 같은 내용 재관측은 스냅샷/관측 행을 늘리지 않는다.
//! - 내용이 바뀌면 같은 세션에 두 번째 관측 행이 생긴다(세션 중 변경 = 집합).
//! - 신선도 가드: 세션의 마지막 관측 시각이 오래됐으면(기본 10분) 기록하지
//!   않는다 — 죽은 세션에 오늘의 CLAUDE.md를 붙이는 오염 방지.
//! - Tier2: 파일 안의 `@path` 참조 파일이 존재하면 source='import'로 스냅샷
//!   (로드 여부는 주장하지 않는다).

use sqlx::sqlite::SqlitePoolOptions;
use sqlx::Row;
use wimcc::db::migrate;
use wimcc::insight::instruction_observe::observe_session_instructions;

async fn pool() -> sqlx::SqlitePool {
    let p = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&p).await.unwrap();
    p
}

/// observed_event 최소 시드 — cwd와 관측 시각만 의미 있다.
async fn seed_activity(pool: &sqlx::SqlitePool, sid: &str, cwd: &str, observed_at: &str) {
    sqlx::query(
        "INSERT OR IGNORE INTO ingest_run (run_id, started_at, status) VALUES ('run_io', datetime('now'), 'done')",
    )
    .execute(pool)
    .await
    .unwrap();
    let raw = format!("raw_io_{sid}_{observed_at}");
    sqlx::query(
        "INSERT INTO raw_event (raw_event_id, ingest_run_id, source_type, source_uri, source_line_no, source_byte_offset, payload_sha256, payload, captured_at)
         VALUES (?, 'run_io', 'claude_transcript', 'io.jsonl', 0, 0, ?, '{}', datetime('now'))",
    )
    .bind(&raw)
    .bind(format!("sha_{raw}"))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO observed_event (event_id, raw_event_id, schema_version, session_id, observed_at, actor, kind, payload, parser_version, cwd)
         VALUES (?, ?, 'observed_event.v1', ?, ?, 'assistant', 'assistant_message', '{}', 'test', ?)",
    )
    .bind(format!("ev_io_{sid}_{observed_at}"))
    .bind(&raw)
    .bind(sid)
    .bind(observed_at)
    .bind(cwd)
    .execute(pool)
    .await
    .unwrap();
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

#[tokio::test]
async fn observes_project_and_user_claude_md_dedups_and_tracks_change() {
    let p = pool().await;
    let dir = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("CLAUDE.md"), "# v1\n").unwrap();
    std::fs::create_dir_all(home.path().join(".claude")).unwrap();
    std::fs::write(home.path().join(".claude/CLAUDE.md"), "# user\n").unwrap();
    seed_activity(&p, "s1", dir.path().to_str().unwrap(), &now_iso()).await;

    let n1 = observe_session_instructions(&p, "s1", Some(home.path()))
        .await
        .unwrap();
    assert_eq!(n1, 2); // project + user

    // 같은 내용 재관측 → 추가 없음 (내용 주소화 + unique)
    let n2 = observe_session_instructions(&p, "s1", Some(home.path()))
        .await
        .unwrap();
    assert_eq!(n2, 0);

    // 내용 변경 → 같은 세션에 새 관측 행 (세션 중 변경 = 집합)
    std::fs::write(dir.path().join("CLAUDE.md"), "# v2\n").unwrap();
    let n3 = observe_session_instructions(&p, "s1", Some(home.path()))
        .await
        .unwrap();
    assert_eq!(n3, 1);

    let rows = sqlx::query(
        "SELECT source, COUNT(DISTINCT content_sha256) c FROM instruction_observation WHERE session_id='s1' GROUP BY source ORDER BY source",
    )
    .fetch_all(&p)
    .await
    .unwrap();
    let m: Vec<(String, i64)> = rows.iter().map(|r| (r.get("source"), r.get("c"))).collect();
    assert_eq!(m, vec![("project".into(), 2), ("user".into(), 1)]);
}

#[tokio::test]
async fn stale_session_is_not_labeled_with_todays_instructions() {
    let p = pool().await;
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("CLAUDE.md"), "# v1\n").unwrap();
    seed_activity(
        &p,
        "s_old",
        dir.path().to_str().unwrap(),
        "2026-06-01T00:00:00+00:00",
    )
    .await;
    let n = observe_session_instructions(&p, "s_old", None)
        .await
        .unwrap();
    assert_eq!(n, 0);
}

#[tokio::test]
async fn tier2_imports_snapshotted_without_load_claim() {
    let p = pool().await;
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("CLAUDE.md"), "규칙은 @docs/rules.md 참고\n").unwrap();
    std::fs::create_dir_all(dir.path().join("docs")).unwrap();
    std::fs::write(dir.path().join("docs/rules.md"), "# rules\n").unwrap();
    seed_activity(&p, "s2", dir.path().to_str().unwrap(), &now_iso()).await;

    observe_session_instructions(&p, "s2", None).await.unwrap();
    let src: Vec<String> = sqlx::query(
        "SELECT DISTINCT source FROM instruction_observation WHERE session_id='s2' ORDER BY source",
    )
    .fetch_all(&p)
    .await
    .unwrap()
    .iter()
    .map(|r| r.get("source"))
    .collect();
    assert_eq!(src, vec!["import".to_string(), "project".to_string()]);
}

#[tokio::test]
async fn shell_cd_drift_does_not_create_project_rows() {
    // 실측(2026-07-04, 세션 190a23db 라이브 스모크): Bash `cd`로 레코드 cwd가
    // 드리프트해 distinct cwd 4개 — 전부를 project 루트로 취급하면 하위
    // CLAUDE.md가 코호트 키('project')로 오기록된다. 기준은 최초 이벤트의
    // cwd(launch dir) 하나다. 하위 파일은 tree(존재 기록)로만 나타난다.
    let p = pool().await;
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("CLAUDE.md"), "# root\n").unwrap();
    std::fs::create_dir_all(dir.path().join("webui")).unwrap();
    std::fs::write(dir.path().join("webui/CLAUDE.md"), "# sub\n").unwrap();
    let earlier = (chrono::Utc::now() - chrono::Duration::minutes(2)).to_rfc3339();
    seed_activity(&p, "s4", dir.path().to_str().unwrap(), &earlier).await;
    seed_activity(
        &p,
        "s4",
        dir.path().join("webui").to_str().unwrap(),
        &now_iso(),
    )
    .await;

    observe_session_instructions(&p, "s4", None).await.unwrap();
    let proj: Vec<String> = sqlx::query(
        "SELECT path FROM instruction_observation WHERE session_id='s4' AND source='project'",
    )
    .fetch_all(&p)
    .await
    .unwrap()
    .iter()
    .map(|r| r.get("path"))
    .collect();
    assert_eq!(proj.len(), 1, "launch cwd의 CLAUDE.md만 project: {proj:?}");
    assert!(!proj[0].ends_with("webui/CLAUDE.md"));
    let tree: Vec<String> = sqlx::query(
        "SELECT path FROM instruction_observation WHERE session_id='s4' AND source='tree'",
    )
    .fetch_all(&p)
    .await
    .unwrap()
    .iter()
    .map(|r| r.get("path"))
    .collect();
    assert_eq!(tree.len(), 1);
    assert!(tree[0].ends_with("webui/CLAUDE.md"));
}

#[tokio::test]
async fn tier3_tree_claude_md_recorded_as_existence_only() {
    let p = pool().await;
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("CLAUDE.md"), "# root\n").unwrap();
    std::fs::create_dir_all(dir.path().join("webui/src")).unwrap();
    std::fs::write(dir.path().join("webui/CLAUDE.md"), "# sub\n").unwrap();
    // 제외 디렉토리의 CLAUDE.md는 스캔하지 않는다.
    std::fs::create_dir_all(dir.path().join("node_modules/x")).unwrap();
    std::fs::write(dir.path().join("node_modules/x/CLAUDE.md"), "# noise\n").unwrap();
    seed_activity(&p, "s3", dir.path().to_str().unwrap(), &now_iso()).await;

    observe_session_instructions(&p, "s3", None).await.unwrap();
    let rows = sqlx::query(
        "SELECT source, path FROM instruction_observation WHERE session_id='s3' AND source='tree'",
    )
    .fetch_all(&p)
    .await
    .unwrap();
    assert_eq!(rows.len(), 1);
    let path: String = rows[0].get("path");
    assert!(path.ends_with("webui/CLAUDE.md"));
}
