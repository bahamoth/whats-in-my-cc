//! S6 (UX 재설계) — `/v1/sessions` 응답이 식별 정보를 함께 싣는다:
//! `slug` · `project`(cwd) · `model`(dominant) · `first_user_message_preview`.
//!
//! 필드 의미 잠금 (real-data anchoring, CLAUDE.md):
//! - `slug`는 transcript의 `system` 요약 라인 top-level `slug` 필드에서 온다 →
//!   `session_facts_v01.jsonl`(frozen real payload)로 invariant 잠금.
//! - `model`(`message.model`→`assistant_message.payload.model`)·`cwd`·user_message
//!   content는 코드베이스 전반에서 이미 검증된 추출이라, 여기선 *집계 로직*
//!   (dominant model 선택·첫 user_message preview)을 인라인 단일 세션으로 잠근다.

use axum_test::TestServer;
use serde_json::Value;
use sqlx::sqlite::SqlitePoolOptions;
use wimcc::api::AppState;
use wimcc::db::migrate;
use wimcc::ingest::store;
use wimcc::live::NoopSink;

async fn fresh_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    pool
}

fn build_server(pool: sqlx::SqlitePool) -> TestServer {
    let state = AppState::new_for_tests(pool);
    TestServer::new(wimcc::api::router(state)).unwrap()
}

async fn ingest_inline(pool: &sqlx::SqlitePool, jsonl: &str) {
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("session.jsonl");
    std::fs::write(&f, jsonl).unwrap();
    store::ingest_file(pool, &f, &NoopSink).await.unwrap();
}

/// 단일 세션: 첫 user 프롬프트, opus 텍스트 응답 ×2, sonnet 텍스트 응답 ×1,
/// 그리고 slug를 담은 system 요약. dominant model은 opus여야 하고 preview는
/// 첫 사용자 메시지여야 한다.
const SESSION_JSONL: &str = concat!(
    r#"{"type":"user","uuid":"u1","parentUuid":null,"sessionId":"sess-facts","timestamp":"2026-06-10T00:00:00Z","cwd":"/tmp/myproj","gitBranch":"main","entrypoint":"cli","userType":"external","version":"2.1.144","isSidechain":false,"isMeta":false,"promptId":"p1","message":{"role":"user","content":"리팩터링 계획을 세워줘"}}"#,
    "\n",
    r#"{"type":"assistant","uuid":"a1","parentUuid":"u1","sessionId":"sess-facts","timestamp":"2026-06-10T00:00:01Z","cwd":"/tmp/myproj","gitBranch":"main","entrypoint":"cli","userType":"external","version":"2.1.144","isSidechain":false,"requestId":"req_a1","message":{"id":"msg_a1","model":"claude-opus-4-8","type":"message","role":"assistant","stop_reason":"end_turn","content":[{"type":"text","text":"먼저 구조를 봅니다"}]}}"#,
    "\n",
    r#"{"type":"assistant","uuid":"a2","parentUuid":"a1","sessionId":"sess-facts","timestamp":"2026-06-10T00:00:02Z","cwd":"/tmp/myproj","gitBranch":"main","entrypoint":"cli","userType":"external","version":"2.1.144","isSidechain":false,"requestId":"req_a2","message":{"id":"msg_a2","model":"claude-opus-4-8","type":"message","role":"assistant","stop_reason":"end_turn","content":[{"type":"text","text":"계획 초안입니다"}]}}"#,
    "\n",
    r#"{"type":"assistant","uuid":"a3","parentUuid":"a2","sessionId":"sess-facts","timestamp":"2026-06-10T00:00:03Z","cwd":"/tmp/myproj","gitBranch":"main","entrypoint":"cli","userType":"external","version":"2.1.144","isSidechain":false,"requestId":"req_a3","message":{"id":"msg_a3","model":"claude-sonnet-4-6","type":"message","role":"assistant","stop_reason":"end_turn","content":[{"type":"text","text":"보조 응답"}]}}"#,
    "\n",
    r#"{"type":"system","subtype":"turn_duration","uuid":"s1","parentUuid":"a3","sessionId":"sess-facts","timestamp":"2026-06-10T00:00:04Z","cwd":"/tmp/myproj","gitBranch":"main","entrypoint":"cli","userType":"external","version":"2.1.144","isSidechain":false,"isMeta":false,"durationMs":4000,"slug":"calm-river-otter"}"#,
    "\n",
);

#[tokio::test]
async fn session_list_surfaces_identity_facts() {
    let pool = fresh_pool().await;
    ingest_inline(&pool, SESSION_JSONL).await;
    let server = build_server(pool);
    let r = server.get("/v1/sessions").await;
    r.assert_status_ok();
    let body: Value = r.json();
    let item = &body["data"][0];
    assert_eq!(item["session_id"], "sess-facts", "single session expected");

    assert_eq!(
        item["project"], "/tmp/myproj",
        "project must be the session cwd; got: {item}"
    );
    assert_eq!(
        item["model"], "claude-opus-4-8",
        "model must be the dominant assistant model (opus ×2 > sonnet ×1); got: {item}"
    );
    assert_eq!(
        item["slug"], "calm-river-otter",
        "slug must come from the system summary payload; got: {item}"
    );
    assert_eq!(
        item["first_user_message_preview"], "리팩터링 계획을 세워줘",
        "preview must be the first user message content; got: {item}"
    );
}

/// preview는 슬래시-커맨드 래퍼(`<command-name>…`, `<local-command-stdout>…`)를
/// 건너뛰고 첫 *실제* 사용자 프롬프트를 고른다 — S6의 목적이 "사람이 읽는 목록"
/// 이므로 `<command-name>/effort</command-name>` 류는 식별에 도움이 안 된다.
#[tokio::test]
async fn preview_skips_slash_command_wrappers() {
    let pool = fresh_pool().await;
    let jsonl = concat!(
        r#"{"type":"user","uuid":"u1","parentUuid":null,"sessionId":"sess-cmd","timestamp":"2026-06-10T00:00:00Z","cwd":"/tmp/p","gitBranch":"main","entrypoint":"cli","userType":"external","version":"2.1.144","isSidechain":false,"isMeta":false,"promptId":"p1","message":{"role":"user","content":"<command-name>/effort</command-name>\n<command-message>effort</command-message>"}}"#,
        "\n",
        r#"{"type":"user","uuid":"u2","parentUuid":"u1","sessionId":"sess-cmd","timestamp":"2026-06-10T00:00:01Z","cwd":"/tmp/p","gitBranch":"main","entrypoint":"cli","userType":"external","version":"2.1.144","isSidechain":false,"isMeta":false,"promptId":"p2","message":{"role":"user","content":"진짜 첫 프롬프트"}}"#,
        "\n",
    );
    ingest_inline(&pool, jsonl).await;
    let server = build_server(pool);
    let r = server.get("/v1/sessions").await;
    r.assert_status_ok();
    let body: Value = r.json();
    let item = &body["data"][0];
    assert_eq!(
        item["first_user_message_preview"], "진짜 첫 프롬프트",
        "preview must skip the slash-command wrapper message; got: {item}"
    );
}

/// perf-2026-06-29 — facets are materialized into the `session_summary` table
/// by `recompute_session` (transcript ingest path) so `/v1/sessions` reads them
/// without re-scanning + json_extract over the whole observed_event table on
/// every request. Lock that the table is populated by an ordinary ingest.
#[tokio::test]
async fn recompute_materializes_session_summary_facets() {
    use sqlx::Row as _;
    let pool = fresh_pool().await;
    ingest_inline(&pool, SESSION_JSONL).await;
    let row = sqlx::query(
        "SELECT project, model, slug, first_user_message_preview \
         FROM session_summary WHERE session_id = 'sess-facts'",
    )
    .fetch_one(&pool)
    .await
    .expect("session_summary row must exist after ingest");
    assert_eq!(row.get::<String, _>("project"), "/tmp/myproj");
    assert_eq!(row.get::<String, _>("model"), "claude-opus-4-8");
    assert_eq!(row.get::<String, _>("slug"), "calm-river-otter");
    assert_eq!(
        row.get::<String, _>("first_user_message_preview"),
        "리팩터링 계획을 세워줘"
    );
}

/// perf-2026-06-29 — pre-migration sessions (ingested before session_summary
/// existed) are backfilled on serve/ingest startup. Simulate by clearing the
/// materialized table after ingest, then backfilling.
#[tokio::test]
async fn backfill_fills_session_summary_for_existing_sessions() {
    let pool = fresh_pool().await;
    ingest_inline(&pool, SESSION_JSONL).await;
    sqlx::query("DELETE FROM session_summary")
        .execute(&pool)
        .await
        .unwrap();
    let n = wimcc::db::repo_observed::backfill_session_summary(&pool)
        .await
        .expect("backfill must succeed");
    assert!(
        n >= 1,
        "backfill must fill at least the one ingested session"
    );
    let model: String =
        sqlx::query_scalar("SELECT model FROM session_summary WHERE session_id = 'sess-facts'")
            .fetch_one(&pool)
            .await
            .expect("backfilled row must exist");
    assert_eq!(model, "claude-opus-4-8");
}

/// slug 의미 잠금: 실 transcript(`session_facts_v01.jsonl`)의 `system`
/// 요약 라인 top-level `slug`가 list 응답으로 surface 된다.
#[tokio::test]
async fn slug_comes_from_real_system_summary_payload() {
    let pool = fresh_pool().await;
    store::ingest_file(
        &pool,
        std::path::Path::new("tests/fixtures/transcripts/real/session_facts_v01.jsonl"),
        &NoopSink,
    )
    .await
    .unwrap();
    let server = build_server(pool);
    let r = server.get("/v1/sessions?limit=50").await;
    r.assert_status_ok();
    let body: Value = r.json();
    let arr = body["data"].as_array().expect("data array");
    let found = arr
        .iter()
        .find(|it| it["session_id"] == "0daafa6e-7a95-4c11-b76d-230208456833");
    let item = found.expect("session 0daafa6e present after ingest");
    assert_eq!(
        item["slug"], "rustling-dazzling-puppy",
        "real system-summary slug must surface in the list; got: {item}"
    );
}

/// B-6a (2026-07-04) — teammate 세션의 preview는 raw `<teammate-message …>`
/// 래퍼가 아니라 그 안의 실제 첫 메시지 본문이어야 한다. real fixture
/// (teammate_v01/teammate_session_head.jsonl, 세션 e8b4a11e)의 첫 user
/// 메시지는 `<teammate-message teammate_id="team-lead">\n{본문}\n</teammate-message>`
/// 형태다 — 래퍼를 벗긴 본문으로 preview를 만든다.
#[tokio::test]
async fn preview_strips_teammate_message_wrapper() {
    let pool = fresh_pool().await;
    store::ingest_file(
        &pool,
        std::path::Path::new(
            "tests/fixtures/transcripts/real/teammate_v01/teammate_session_head.jsonl",
        ),
        &NoopSink,
    )
    .await
    .unwrap();
    let server = build_server(pool);
    let r = server.get("/v1/sessions").await;
    r.assert_status_ok();
    let body: Value = r.json();
    let item = &body["data"][0];
    let preview = item["first_user_message_preview"].as_str().unwrap_or("");
    assert!(
        !preview.starts_with("<teammate-message"),
        "preview must not expose the raw teammate-message wrapper; got: {preview}"
    );
    assert!(
        preview.starts_with("/Users/bahamoth/projects/whats-in-my-cc 저장소의"),
        "preview must be the inner dispatch text; got: {preview}"
    );
}

/// B-6a 보강 — 리드 세션 쪽 relayed 형태("Another Claude session sent a
/// message: <teammate-message …>")도 raw XML을 preview에 노출하지 않는다.
/// real fixture: teammate_v01/lead_teammate_messages.jsonl (관측 창이 relayed
/// 메시지로 시작하는 리드 세션 — 두 마커 형태는 messageOrigin.ts와 동일 실측).
#[tokio::test]
async fn preview_strips_relayed_teammate_message_wrapper() {
    let pool = fresh_pool().await;
    store::ingest_file(
        &pool,
        std::path::Path::new(
            "tests/fixtures/transcripts/real/teammate_v01/lead_teammate_messages.jsonl",
        ),
        &NoopSink,
    )
    .await
    .unwrap();
    let server = build_server(pool);
    let r = server.get("/v1/sessions").await;
    r.assert_status_ok();
    let body: Value = r.json();
    let item = &body["data"][0];
    let preview = item["first_user_message_preview"].as_str().unwrap_or("");
    assert!(
        !preview.contains("<teammate-message"),
        "relayed teammate wrapper must not leak into preview; got: {preview}"
    );
}

/// B-6e (2026-07-04) — teamName "session-<리드 8자>" 형태 표본 2 확보.
/// 이 세션(190a23db, CC 2.1.200)이 직접 스폰해 동결한 real fixture
/// (teammate_v02/named_teammate_head.jsonl): agentName=sample-probe,
/// teamName=session-190a23db, agent-setting=general-purpose(타입 그대로 —
/// Explore 외 값의 첫 표본). 같은 라운드에 클래식(이름 없는) 서브에이전트가
/// 2.1.200에서 여전히 사이드카임도 실측(classic_sidecar_head_2_1_200.jsonl).
#[tokio::test]
async fn teammate_v02_second_sample_locks_team_join_shape() {
    let pool = fresh_pool().await;
    store::ingest_file(
        &pool,
        std::path::Path::new(
            "tests/fixtures/transcripts/real/teammate_v02/named_teammate_head.jsonl",
        ),
        &NoopSink,
    )
    .await
    .unwrap();
    let server = build_server(pool);
    let r = server.get("/v1/sessions").await;
    let body: Value = r.json();
    let item = &body["data"][0];
    assert_eq!(item["agent_name"], "sample-probe");
    assert_eq!(item["team_name"], "session-190a23db");
}
