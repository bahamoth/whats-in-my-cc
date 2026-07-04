//! 세션 환경 fingerprint — "이 세션은 어떤 구성 아래에서 돌았는가"의 결정론 관측.
//!
//! 자기개선 루프의 독립변수: 개입(스킬/모델 변경)의 효과를 세션 코호트로 귀속하려면
//! 구성의 관측이 선행해야 한다. SessionMetrics와 같은 on-demand 무저장 패턴(§10.1) —
//! 관측 값만, 판단 필드 없음(§6.3).
//!
//! 출처: `models`는 assistant payload `/model`(payload 필드라 과거 세션은 재ingest
//! 필요), `cc_versions`/`git_branches`/`cwds`/`entrypoints`는 observed_event 컬럼.
//!
//! (CLAUDE.md instruction 스냅샷 필드 `claude_md`/`instruction_sha256`은 collector hook
//! 의존이라 2026-06-19 제거 — collector forward 폐지 + 단일 사용자 환경에서 git 이력으로
//! 대체 가능해 효용 없음. 근거: docs/implementation-notes.html.)

use serde::Serialize;
use sqlx::{Row, SqlitePool};

use crate::error::Result;

#[derive(Debug, Clone, Serialize)]
pub struct SessionFingerprint {
    pub session_id: String,
    /// assistant_message payload `/model`의 distinct 정렬 목록.
    pub models: Vec<String>,
    pub cc_versions: Vec<String>,
    pub git_branches: Vec<String>,
    pub cwds: Vec<String>,
    pub entrypoints: Vec<String>,
    /// 4차 개정 — tool_call tool_name(`mcp__…`)에서 파생한 관측 MCP
    /// server_key 집합(정렬). 개입 차원: "이 플러그인을 붙인 뒤 달라졌나".
    pub plugins: Vec<String>,
    /// instruction 전향 관측 — (source, content_sha256) distinct 목록.
    /// serve 가동 중 관측된 세션에만 존재(소급 backfill 없음).
    pub instructions: Vec<InstructionRef>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstructionRef {
    pub source: String,
    pub hash: String,
}

/// observed_event의 nullable TEXT 컬럼 하나의 distinct 정렬 목록.
/// `col`은 아래 호출부의 고정 식별자만 — 사용자 입력이 아니다.
async fn distinct_column(pool: &SqlitePool, session_id: &str, col: &str) -> Result<Vec<String>> {
    let sql = format!(
        "SELECT DISTINCT {col} AS v FROM observed_event \
         WHERE session_id = ? AND {col} IS NOT NULL ORDER BY v"
    );
    let rows = sqlx::query(&sql).bind(session_id).fetch_all(pool).await?;
    Ok(rows.iter().map(|r| r.get::<String, _>("v")).collect())
}

pub async fn compute_session_fingerprint(
    pool: &SqlitePool,
    session_id: &str,
) -> Result<SessionFingerprint> {
    let models_rows = sqlx::query(
        "SELECT DISTINCT json_extract(payload, '$.model') AS m FROM observed_event \
         WHERE session_id = ? AND kind = 'assistant_message' \
           AND json_extract(payload, '$.model') IS NOT NULL ORDER BY m",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?;
    let models = models_rows
        .iter()
        .map(|r| r.get::<String, _>("m"))
        .collect();

    let tool_rows = sqlx::query(
        "SELECT DISTINCT json_extract(payload, '$.tool_name') AS t FROM observed_event \
         WHERE session_id = ? AND kind = 'tool_call' \
           AND json_extract(payload, '$.tool_name') LIKE 'mcp__%'",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?;
    let plugins: Vec<String> = tool_rows
        .iter()
        .filter_map(|r| {
            let name: String = r.get("t");
            crate::insight::event_tags::parse_mcp_tool(&name).map(|(k, _)| k.to_string())
        })
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();

    let instr_rows = sqlx::query(
        "SELECT DISTINCT source, content_sha256 FROM instruction_observation \
         WHERE session_id = ? ORDER BY source, content_sha256",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?;
    let instructions = instr_rows
        .iter()
        .map(|r| InstructionRef {
            source: r.get("source"),
            hash: r.get("content_sha256"),
        })
        .collect();

    Ok(SessionFingerprint {
        session_id: session_id.to_string(),
        models,
        cc_versions: distinct_column(pool, session_id, "cc_version").await?,
        git_branches: distinct_column(pool, session_id, "git_branch").await?,
        cwds: distinct_column(pool, session_id, "cwd").await?,
        entrypoints: distinct_column(pool, session_id, "entrypoint").await?,
        plugins,
        instructions,
    })
}
