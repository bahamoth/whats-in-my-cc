//! S8 (UX 재설계) — `/v1/sessions/:id/turns` 의 각 턴에 per-turn 토큰 합계를
//! 실어 KPI 스파크라인(intra-session 추이)을 그릴 수 있게 한다.
//!
//! per-turn 토큰 = `usage_facet ⋈ observed_event (raw_event_id) GROUP BY turn_id`.
//! usage는 assistant 라인의 `message.usage`에서 ingest 시 usage_facet로 추출되고,
//! turn_id는 `promptId`에서 온다(둘 다 코드베이스에 이미 검증된 추출). 여기선
//! *턴 단위 합산*을 잠근다.

use axum_test::TestServer;
use serde_json::Value;
use sqlx::sqlite::SqlitePoolOptions;
use wimcc::api::AppState;
use wimcc::db::migrate;
use wimcc::ingest::store;
use wimcc::live::NoopSink;

async fn ingest(jsonl: &str) -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("s.jsonl");
    std::fs::write(&f, jsonl).unwrap();
    store::ingest_file(&pool, &f, &NoopSink).await.unwrap();
    pool
}

// 2 turns (promptId p1, p2). Each: a user message + an assistant message whose
// `message.usage` carries the turn's tokens.
const JSONL: &str = concat!(
    r#"{"type":"user","uuid":"u1","parentUuid":null,"sessionId":"sess-tok","timestamp":"2026-06-10T00:00:00Z","cwd":"/tmp","gitBranch":"main","entrypoint":"cli","userType":"external","version":"2.1.144","isSidechain":false,"isMeta":false,"promptId":"p1","message":{"role":"user","content":"첫 턴"}}"#,
    "\n",
    r#"{"type":"assistant","uuid":"a1","parentUuid":"u1","sessionId":"sess-tok","timestamp":"2026-06-10T00:00:01Z","cwd":"/tmp","gitBranch":"main","entrypoint":"cli","userType":"external","version":"2.1.144","isSidechain":false,"promptId":"p1","requestId":"req_a1","message":{"id":"msg_a1","model":"claude-opus-4-8","type":"message","role":"assistant","stop_reason":"end_turn","usage":{"input_tokens":100,"cache_creation_input_tokens":200,"cache_read_input_tokens":1000,"output_tokens":50},"content":[{"type":"text","text":"하나"}]}}"#,
    "\n",
    r#"{"type":"user","uuid":"u2","parentUuid":"a1","sessionId":"sess-tok","timestamp":"2026-06-10T00:00:02Z","cwd":"/tmp","gitBranch":"main","entrypoint":"cli","userType":"external","version":"2.1.144","isSidechain":false,"isMeta":false,"promptId":"p2","message":{"role":"user","content":"둘째 턴"}}"#,
    "\n",
    r#"{"type":"assistant","uuid":"a2","parentUuid":"u2","sessionId":"sess-tok","timestamp":"2026-06-10T00:00:03Z","cwd":"/tmp","gitBranch":"main","entrypoint":"cli","userType":"external","version":"2.1.144","isSidechain":false,"promptId":"p2","requestId":"req_a2","message":{"id":"msg_a2","model":"claude-opus-4-8","type":"message","role":"assistant","stop_reason":"end_turn","usage":{"input_tokens":10,"cache_creation_input_tokens":0,"cache_read_input_tokens":500,"output_tokens":20},"content":[{"type":"text","text":"둘"}]}}"#,
    "\n",
);

#[tokio::test]
async fn turns_carry_per_turn_token_sums() {
    let pool = ingest(JSONL).await;
    let server = TestServer::new(wimcc::api::router(AppState::new_for_tests(pool))).unwrap();
    let r = server.get("/v1/sessions/sess-tok/turns").await;
    r.assert_status_ok();
    let body: Value = r.json();
    let turns = body["data"]["turns"].as_array().expect("turns array");
    assert_eq!(
        turns.len(),
        2,
        "two prompt turns expected; got: {}",
        turns.len()
    );

    let t0 = &turns[0]["tokens"];
    assert_eq!(t0["input_tokens"], 100, "turn 1 input; got: {}", turns[0]);
    assert_eq!(
        t0["cache_creation_input_tokens"], 200,
        "turn 1 cache_creation"
    );
    assert_eq!(t0["cache_read_input_tokens"], 1000, "turn 1 cache_read");
    assert_eq!(t0["output_tokens"], 50, "turn 1 output");

    let t1 = &turns[1]["tokens"];
    assert_eq!(t1["input_tokens"], 10, "turn 2 input; got: {}", turns[1]);
    assert_eq!(t1["output_tokens"], 20, "turn 2 output");
}
