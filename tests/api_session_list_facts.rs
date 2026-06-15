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
