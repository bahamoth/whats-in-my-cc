//! 세션 환경 fingerprint — 개입(구성) 귀속의 독립변수 (Task 3).
//!
//! SessionMetrics와 같은 on-demand 무저장 패턴. 모든 Vec은 정렬·distinct.
//! 판단 필드 없음 — 관측 값만.
//! (claude_md/instruction_sha256 필드는 hook collector 폐지로 2026-06-19 제거.)

use serde_json::json;
use sqlx::sqlite::SqlitePoolOptions;
use wimcc::db::{migrate, repo_observed, repo_raw, repo_runs};
use wimcc::insight::fingerprint::compute_session_fingerprint;
use wimcc::model::observed::{Actor, EventKind, ObservedEvent};

async fn test_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    pool
}

#[allow(clippy::too_many_arguments)]
async fn seed(
    pool: &sqlx::SqlitePool,
    run_id: &str,
    sid: &str,
    eid: &str,
    kind: EventKind,
    subkind: Option<&str>,
    payload: serde_json::Value,
    cc_version: Option<&str>,
    git_branch: Option<&str>,
) {
    let raw_id = format!("raw_{eid}");
    repo_raw::insert_dedup(
        pool,
        &repo_raw::NewRaw {
            raw_event_id: raw_id.clone(),
            ingest_run_id: run_id.into(),
            source_type: "claude_transcript".into(),
            source_uri: "/tmp/fp.jsonl".into(),
            source_line_no: 0,
            source_byte_offset: 0,
            payload_sha256: format!("sha_{eid}"),
            payload: b"{}".to_vec(),
            parse_error: None,
            captured_at: chrono::Utc::now(),
            redaction_state: "not_applicable".into(),
            redaction_manifest: None,
        },
    )
    .await
    .unwrap();
    let e = ObservedEvent {
        event_id: eid.into(),
        raw_event_id: raw_id,
        schema_version: "observed_event.v1".into(),
        session_id: sid.into(),
        observed_at: chrono::Utc::now(),
        actor: Actor::Assistant,
        kind,
        subkind: subkind.map(String::from),
        payload,
        cwd: Some("/proj/x".into()),
        entrypoint: Some("cli".into()),
        cc_version: cc_version.map(String::from),
        git_branch: git_branch.map(String::from),
        parser_version: "test@v0".into(),
        ..Default::default()
    };
    repo_observed::insert(pool, &e).await.unwrap();
}

#[tokio::test]
async fn fingerprint_collects_distinct_sorted_env_and_models() {
    let pool = test_pool().await;
    let run = repo_runs::start(&pool).await.unwrap();
    let sid = "sess_fp_1";
    seed(
        &pool,
        &run,
        sid,
        "a1",
        EventKind::AssistantMessage,
        None,
        json!({"model": "claude-opus-4-7"}),
        Some("2.1.0"),
        Some("main"),
    )
    .await;
    seed(
        &pool,
        &run,
        sid,
        "a2",
        EventKind::AssistantMessage,
        None,
        json!({"model": "claude-opus-4-7"}),
        Some("2.1.0"),
        Some("main"),
    )
    .await;
    seed(
        &pool,
        &run,
        sid,
        "a3",
        EventKind::AssistantMessage,
        None,
        json!({"model": "claude-haiku-4-5"}),
        Some("2.1.1"),
        Some("feat/x"),
    )
    .await;
    // 모델 없는 user 이벤트는 models에 영향을 주지 않아야 한다.
    seed(
        &pool,
        &run,
        sid,
        "u1",
        EventKind::UserMessage,
        None,
        json!({"text": "hi"}),
        Some("2.1.0"),
        Some("main"),
    )
    .await;

    let fp = compute_session_fingerprint(&pool, sid).await.unwrap();
    assert_eq!(fp.session_id, sid);
    assert_eq!(fp.models, vec!["claude-haiku-4-5", "claude-opus-4-7"]);
    assert_eq!(fp.cc_versions, vec!["2.1.0", "2.1.1"]);
    assert_eq!(fp.git_branches, vec!["feat/x", "main"]);
    assert_eq!(fp.cwds, vec!["/proj/x"]);
    assert_eq!(fp.entrypoints, vec!["cli"]);
}

#[tokio::test]
async fn fingerprint_empty_session_is_all_empty() {
    let pool = test_pool().await;
    let fp = compute_session_fingerprint(&pool, "sess_fp_none")
        .await
        .unwrap();
    assert!(fp.models.is_empty());
    assert!(fp.cc_versions.is_empty());
    assert!(fp.git_branches.is_empty());
    assert!(fp.cwds.is_empty());
    assert!(fp.entrypoints.is_empty());
}
