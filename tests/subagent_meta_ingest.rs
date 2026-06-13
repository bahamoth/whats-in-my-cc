//! Subagent 사이드카 meta.json ingest — 호출 관계(agentId ↔ Task tool_use_id)의 원본.
//!
//! 실 fixture: `tests/fixtures/transcripts/real/subagent_sidecar_v01/` —
//! 2026-06-13 원격 세션(CC 2.1.176, entrypoint remote_mobile)에서 동결한 실
//! payload, **표본 1**. 관측된 구조:
//!   `<sessionId>/subagents/agent-<agentId>.jsonl`  (sidechain 레코드, agentId 포함)
//!   `<sessionId>/subagents/agent-<agentId>.meta.json`
//!     = `{agentType, description, toolUseId}` — toolUseId가 메인 체인 Task
//!       tool_use id와 일치한다. meta.json 자체에는 sessionId·agentId가 없어
//!       경로(부모의 부모 디렉터리명)와 파일명에서 끌어낸다.
//! 로컬 CC에서도 같은 구조인지는 미확인(원격 표본 1) — 부재 시 degrade가 원칙.

use serde_json::Value;
use sqlx::sqlite::SqlitePoolOptions;
use std::path::Path;
use wimcc::db::migrate;
use wimcc::ingest::store::ingest_paths;
use wimcc::ingest::subagent_meta::sidecar_path_parts;
use wimcc::live::NoopSink;

const FIXTURE_DIR: &str =
    "tests/fixtures/transcripts/real/subagent_sidecar_v01/182d7259-2834-5e3d-ace1-8c9951578fc6/subagents";
const SESSION: &str = "182d7259-2834-5e3d-ace1-8c9951578fc6";
const AGENT: &str = "a41279d112a0f8be5";
const TASK_TOOL_USE: &str = "toolu_01EWxShH379EhnfNU8anszj7";

async fn test_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    pool
}

#[test]
fn sidecar_path_parts_extracts_session_and_agent_from_real_layout() {
    let p = Path::new(FIXTURE_DIR).join(format!("agent-{AGENT}.meta.json"));
    let parts = sidecar_path_parts(&p).expect("real layout must parse");
    assert_eq!(parts.session_id, SESSION);
    assert_eq!(parts.agent_id, AGENT);
    // non-sidecar shapes are rejected
    assert!(sidecar_path_parts(Path::new("/x/subagents/agent-a.jsonl")).is_none());
    assert!(sidecar_path_parts(Path::new("/x/other/agent-a.meta.json")).is_none());
    assert!(sidecar_path_parts(Path::new("/x/subagents/foo.meta.json")).is_none());
}

#[test]
fn discover_files_includes_sidecar_meta_next_to_jsonl() {
    let root = Path::new("tests/fixtures/transcripts/real/subagent_sidecar_v01");
    let files = wimcc::ingest::discover_files(root);
    let names: Vec<String> = files
        .iter()
        .filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(String::from))
        .collect();
    assert!(
        names.contains(&format!("agent-{AGENT}.jsonl")),
        "jsonl discovered: {names:?}"
    );
    assert!(
        names.contains(&format!("agent-{AGENT}.meta.json")),
        "meta.json sidecar discovered: {names:?}"
    );
}

#[tokio::test]
async fn sidecar_ingests_as_subagent_meta_event_with_correlation_keys() {
    let pool = test_pool().await;
    let meta_path = Path::new(FIXTURE_DIR).join(format!("agent-{AGENT}.meta.json"));
    let stats = ingest_paths(&pool, &[meta_path.clone()], &NoopSink)
        .await
        .unwrap();
    assert_eq!(stats.raw_inserted, 1);
    assert_eq!(stats.observed_inserted, 1);
    assert!(stats.sessions_touched.contains(SESSION));

    let evs = wimcc::db::repo_observed::list_session(&pool, SESSION, 10)
        .await
        .unwrap();
    assert_eq!(evs.len(), 1);
    let e = &evs[0];
    assert_eq!(e.kind.as_str(), "attachment_meta");
    assert_eq!(e.subkind.as_deref(), Some("subagent_meta"));
    // correlation keys: 그룹 조인용 agent_id + 호출자 점프용 tool_use_id
    assert_eq!(e.agent_id.as_deref(), Some(AGENT));
    assert_eq!(e.tool_use_id.as_deref(), Some(TASK_TOOL_USE));
    assert!(e.is_sidechain);
    // payload는 원본 JSON 전체 보존 (unknown field 포함 원칙)
    let p: &Value = &e.payload;
    assert_eq!(p["agentType"], "Explore");
    assert_eq!(p["description"], "Trivial probe subagent");
    assert_eq!(p["toolUseId"], TASK_TOOL_USE);

    // 재실행은 dedup — observed가 불어나지 않는다
    let stats2 = ingest_paths(&pool, &[meta_path], &NoopSink).await.unwrap();
    assert_eq!(stats2.raw_inserted, 0);
    assert_eq!(stats2.raw_skipped, 1);
    assert_eq!(stats2.observed_inserted, 0);
}

#[tokio::test]
async fn subagent_jsonl_assistant_carries_attribution_agent_in_payload() {
    // 실 fixture의 assistant 레코드 top-level `attributionAgent: "Explore"`
    // (agent 타입)가 normalized payload에 실린다 — meta.json이 유실된 세션에서도
    // agent 타입을 복구할 수 있는 2차 증거. payload 필드라 기존 DB는 재ingest 필요.
    let pool = test_pool().await;
    let jsonl_path = Path::new(FIXTURE_DIR).join(format!("agent-{AGENT}.jsonl"));
    ingest_paths(&pool, &[jsonl_path], &NoopSink).await.unwrap();
    let evs = wimcc::db::repo_observed::list_session(&pool, SESSION, 100)
        .await
        .unwrap();
    let am = evs
        .iter()
        .find(|e| e.kind.as_str() == "assistant_message")
        .expect("assistant_message from fixture");
    assert_eq!(am.payload["attribution_agent"], "Explore");
    assert_eq!(am.agent_id.as_deref(), Some(AGENT));
    // user 레코드에는 attributionAgent가 없다(실측) — 키 자체가 없어야 한다
    let um = evs
        .iter()
        .find(|e| e.kind.as_str() == "user_message")
        .expect("user_message from fixture");
    assert!(um.payload.get("attribution_agent").is_none());
}
