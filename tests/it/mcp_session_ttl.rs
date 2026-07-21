//! growth-2026-07-18 — MCP SessionRegistry idle-TTL eviction.
//!
//! 감사 결과: 레지스트리는 initialize마다 insert만 하고 remove/TTL이 전무해
//! 재연결이 반복되면 세션+broadcast 채널이 프로세스 수명 동안 단조 증가했다
//! (재시작만이 정리 수단). idle TTL을 넘긴 세션은 다음 insert 때 지연
//! 정리되고, 요청 경로의 exists/subscribe 접근이 last_seen을 갱신한다.
//! MCP Streamable HTTP 계약상 세션 소멸은 정상 사건이다 — 클라이언트는
//! unknown session 응답을 받으면 재-initialize한다.

use std::time::Duration;
use wimcc::api::mcp::{McpSession, SessionRegistry};

#[tokio::test]
async fn idle_sessions_are_evicted_on_insert() {
    let reg = SessionRegistry::with_ttl(Duration::ZERO);
    reg.insert(McpSession::new("s1".into())).await;
    reg.insert(McpSession::new("s2".into())).await;
    assert!(
        !reg.exists("s1").await,
        "idle session must be evicted by the next insert"
    );
    assert!(reg.exists("s2").await, "the fresh session stays");
}

#[tokio::test]
async fn accessed_sessions_survive_eviction() {
    let reg = SessionRegistry::with_ttl(Duration::from_millis(500));
    reg.insert(McpSession::new("touched".into())).await;
    reg.insert(McpSession::new("silent".into())).await;

    tokio::time::sleep(Duration::from_millis(300)).await;
    // 요청 경로 접근 = liveness 신호.
    assert!(reg.exists("touched").await);

    tokio::time::sleep(Duration::from_millis(300)).await;
    reg.insert(McpSession::new("s3".into())).await;

    assert!(
        reg.exists("touched").await,
        "session accessed 300ms ago (< 500ms TTL) must survive"
    );
    assert!(
        !reg.exists("silent").await,
        "session idle for 600ms (> 500ms TTL) must be evicted"
    );
}

/// 기본 레지스트리(운영 TTL)는 방금 만든 세션을 절대 지우지 않는다 —
/// 기존 initialize→요청 흐름 회귀 방지.
#[tokio::test]
async fn default_registry_keeps_fresh_sessions() {
    let reg = SessionRegistry::new();
    reg.insert(McpSession::new("a".into())).await;
    reg.insert(McpSession::new("b".into())).await;
    assert!(reg.exists("a").await);
    assert!(reg.exists("b").await);
}
