//! Teammate 세션 필드 정규화 (2026-07-03).
//!
//! CC 2.1.198부터 Agent 툴의 named 스폰은 별도 최상위 세션("teammate")을
//! 만든다 — 자기 sessionId + envelope 필드 `agentName`/`teamName`. 실측:
//! 이 저장소 자신의 리드 세션(bebd8197)이 스폰한 explore-api 세션에서 동결한
//! `tests/fixtures/transcripts/real/teammate_v01/` (표본 1 세션, CC 2.1.198).
//! 세션 목록 그룹핑·리드 조인을 위해 두 필드를 correlation 컬럼으로 승격한다.
//!
//! 관측 invariant (표본 1 세션 내): user/assistant/attachment/system 레코드
//! 전부에 (agentName, teamName)이 동일 상수로 붙는다. agent-setting/mode/
//! permission-mode/last-prompt 레코드에는 없다.

use axum_test::TestServer;
use serde_json::Value;
use sqlx::sqlite::SqlitePoolOptions;
use wimcc::db::migrate;
use wimcc::ingest::store;

const FIXTURE: &str = "tests/fixtures/transcripts/real/teammate_v01/teammate_session_head.jsonl";
const TEAMMATE_SESSION: &str = "e8b4a11e-541d-4d64-9aae-52663c01c5cc";

async fn ingest_fixture() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    store::ingest_file(&pool, std::path::Path::new(FIXTURE), &wimcc::live::NoopSink)
        .await
        .unwrap();
    pool
}

#[tokio::test]
async fn teammate_fields_promoted_to_columns() {
    let pool = ingest_fixture().await;
    // fixture의 user·assistant 레코드는 모두 agentName/teamName을 단다 —
    // 대화 kind 관측 행 전부에 컬럼이 채워져야 한다.
    let rows: Vec<(String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT kind, agent_name, team_name FROM observed_event \
         WHERE session_id = ? AND kind IN ('user_message','assistant_message','tool_call','thinking')",
    )
    .bind(TEAMMATE_SESSION)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(!rows.is_empty(), "fixture must yield conversation events");
    for (kind, agent_name, team_name) in &rows {
        assert_eq!(
            agent_name.as_deref(),
            Some("explore-api"),
            "{kind}: agent_name must be promoted"
        );
        assert_eq!(
            team_name.as_deref(),
            Some("session-bebd8197"),
            "{kind}: team_name must be promoted"
        );
    }
}

#[tokio::test]
async fn session_list_exposes_team_fields() {
    let pool = ingest_fixture().await;
    let state = wimcc::api::AppState::new_for_tests(pool);
    let server = TestServer::new(wimcc::api::router(state)).unwrap();
    let r = server.get("/v1/sessions").await;
    r.assert_status_ok();
    let body: Value = r.json();
    let item = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["session_id"] == TEAMMATE_SESSION)
        .expect("teammate session must be listed");
    assert_eq!(item["agent_name"], "explore-api");
    assert_eq!(item["team_name"], "session-bebd8197");
}

#[tokio::test]
async fn events_dto_exposes_team_fields() {
    let pool = ingest_fixture().await;
    let state = wimcc::api::AppState::new_for_tests(pool);
    let server = TestServer::new(wimcc::api::router(state)).unwrap();
    let r = server
        .get(&format!("/v1/sessions/{TEAMMATE_SESSION}/events"))
        .await;
    r.assert_status_ok();
    let body: Value = r.json();
    let events = body["data"]["events"].as_array().unwrap();
    let ev = events
        .iter()
        .find(|e| e["kind"] == "user_message")
        .expect("user_message event expected");
    assert_eq!(ev["agent_name"], "explore-api");
    assert_eq!(ev["team_name"], "session-bebd8197");
}

/// 기존 DB 경로 (2026-07-03 실사용에서 발견): `init-db`는 데이터를 지우지
/// 않으므로 0026 적용 후 `ingest --all`은 raw UNIQUE dedup으로 전량 스킵되고
/// 기존 관측 행의 팀 컬럼은 NULL로 남는다. raw payload에는 원본 envelope
/// 필드가 보존돼 있으므로 backfill_agent_id 선례대로 startup backfill이
/// 컬럼과 session_summary facet을 채워야 한다.
#[tokio::test]
async fn backfill_fills_team_fields_from_raw() {
    let pool = ingest_fixture().await;
    // 0026 이전 ingest 상태 재현: 관측 컬럼·summary facet을 NULL로 되돌린다.
    sqlx::query("UPDATE observed_event SET agent_name = NULL, team_name = NULL")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE session_summary SET agent_name = NULL, team_name = NULL")
        .execute(&pool)
        .await
        .unwrap();

    let n = wimcc::db::repo_observed::backfill_team_fields(&pool)
        .await
        .unwrap();
    assert!(n > 0, "backfill must touch the nulled rows");

    let (agent, team): (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT agent_name, team_name FROM observed_event \
         WHERE session_id = ? AND kind = 'user_message' LIMIT 1",
    )
    .bind(TEAMMATE_SESSION)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(agent.as_deref(), Some("explore-api"));
    assert_eq!(team.as_deref(), Some("session-bebd8197"));

    // 목록이 읽는 session_summary facet까지 복구돼야 한다.
    let (agent, team): (Option<String>, Option<String>) =
        sqlx::query_as("SELECT agent_name, team_name FROM session_summary WHERE session_id = ?")
            .bind(TEAMMATE_SESSION)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(agent.as_deref(), Some("explore-api"));
    assert_eq!(team.as_deref(), Some("session-bebd8197"));
}

/// 리드 세션 쪽 실측 고정: 리드 transcript의 teammate 응답은 type:user +
/// content 문자열이 "Another Claude session sent a message:\n<teammate-message …>"
/// 형태이고 isMeta가 없다(사람 입력과 구조 신호로만 구분 가능 — webui
/// messageOrigin이 이 마커로 분류한다). 리드 레코드에는 agentName/teamName이
/// 없다는 것도 함께 잠근다 (조인은 팀메이트→리드 단방향).
#[test]
fn lead_teammate_message_shape_invariants() {
    let raw = std::fs::read_to_string(
        "tests/fixtures/transcripts/real/teammate_v01/lead_teammate_messages.jsonl",
    )
    .unwrap();
    let mut checked = 0;
    for line in raw.lines() {
        let r: Value = serde_json::from_str(line).unwrap();
        assert_eq!(r["type"], "user");
        assert!(
            r.get("agentName").is_none(),
            "lead records carry no agentName"
        );
        assert!(
            r.get("teamName").is_none(),
            "lead records carry no teamName"
        );
        let content = r["message"]["content"].as_str().unwrap();
        assert!(content.starts_with("Another Claude session sent a message:"));
        assert!(content.contains("<teammate-message teammate_id=\""));
        checked += 1;
    }
    assert!(checked >= 1, "at least one frozen lead record");
}
