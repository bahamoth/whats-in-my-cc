//! 세션 환경 fingerprint — "이 세션은 어떤 구성 아래에서 돌았는가"의 결정론 관측.
//!
//! 자기개선 루프의 독립변수: 개입(CLAUDE.md/스킬/모델 변경)의 효과를 세션
//! 코호트로 귀속하려면 구성의 관측이 선행해야 한다. SessionMetrics와 같은
//! on-demand 무저장 패턴(§10.1) — 관측 값만, 판단 필드 없음(§6.3).
//!
//! 출처: `models`는 assistant payload `/model`(payload 필드라 과거 세션은
//! 재ingest 필요), `cc_versions`/`git_branches`/`cwds`/`entrypoints`는
//! observed_event 컬럼, `claude_md`는 session_start hook의
//! `/captured/claude_md`(ingest::instruction_snapshot). transcript에는
//! CLAUDE.md가 기록되지 않으므로(2026-06-12 실측) hook 미수집 세션은
//! `claude_md`가 비고 `instruction_sha256`은 None — 결측은 결측으로 노출한다.

use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};

use crate::error::Result;
use crate::ingest::instruction_snapshot::InstructionFile;

#[derive(Debug, Clone, Serialize)]
pub struct SessionFingerprint {
    pub session_id: String,
    /// assistant_message payload `/model`의 distinct 정렬 목록.
    pub models: Vec<String>,
    pub cc_versions: Vec<String>,
    pub git_branches: Vec<String>,
    pub cwds: Vec<String>,
    pub entrypoints: Vec<String>,
    /// session_start 스냅샷의 (path, sha256, bytes) union — path 정렬.
    pub claude_md: Vec<InstructionFile>,
    /// claude_md의 정렬 (path, sha) 결합 sha256 — 코호트 group key.
    /// 스냅샷이 없으면 None (결측 ≠ 빈 instruction).
    pub instruction_sha256: Option<String>,
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

    let snap_rows = sqlx::query(
        "SELECT payload FROM observed_event \
         WHERE session_id = ? AND kind = 'hook_event' AND subkind = 'session_start'",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?;
    let mut claude_md: Vec<InstructionFile> = Vec::new();
    for row in &snap_rows {
        let payload: serde_json::Value =
            serde_json::from_str(&row.get::<String, _>("payload")).unwrap_or_default();
        let Some(files) = payload
            .pointer("/captured/claude_md")
            .and_then(|v| v.as_array())
        else {
            continue;
        };
        for f in files {
            let (Some(path), Some(sha)) = (f["path"].as_str(), f["sha256"].as_str()) else {
                continue;
            };
            let entry = InstructionFile {
                path: path.to_string(),
                sha256: sha.to_string(),
                bytes: f["bytes"].as_u64().unwrap_or(0),
            };
            // union dedup — 세션 재개 등으로 같은 스냅샷이 재수신될 수 있다.
            if !claude_md
                .iter()
                .any(|e| e.path == entry.path && e.sha256 == entry.sha256)
            {
                claude_md.push(entry);
            }
        }
    }
    claude_md.sort_by(|a, b| a.path.cmp(&b.path).then(a.sha256.cmp(&b.sha256)));
    let instruction_sha256 = if claude_md.is_empty() {
        None
    } else {
        let mut h = Sha256::new();
        for f in &claude_md {
            h.update(f.path.as_bytes());
            h.update(b"\n");
            h.update(f.sha256.as_bytes());
            h.update(b"\n");
        }
        Some(hex::encode(h.finalize()))
    };

    Ok(SessionFingerprint {
        session_id: session_id.to_string(),
        models,
        cc_versions: distinct_column(pool, session_id, "cc_version").await?,
        git_branches: distinct_column(pool, session_id, "git_branch").await?,
        cwds: distinct_column(pool, session_id, "cwd").await?,
        entrypoints: distinct_column(pool, session_id, "entrypoint").await?,
        claude_md,
        instruction_sha256,
    })
}
