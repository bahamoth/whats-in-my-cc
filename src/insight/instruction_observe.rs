//! instruction 전향 관측 (스펙 §2 4차 개정).
//!
//! 설계 원칙:
//! - **복원하지 않고 관측한다**: transcript에는 CLAUDE.md 내용이 기록되지 않고
//!   (2026-07-04 전 transcript 실측), git은 dirty tree를 모른다. serve가 세션
//!   활동을 수신한 그 순간 파일을 직접 읽는 것만이 measured다. 소급 backfill
//!   없음 — 과거 세션은 미측정으로 남는다.
//! - **내용 주소화**: 해시는 경계 검출 키, 내용이 본체(경계 diff 렌더의 원료).
//!   같은 내용은 스냅샷 1행, 재관측은 unique index가 흡수한다.
//! - **로드 무주장**: CC의 지시문 로딩 규칙을 재구현하지 않는다. Tier1
//!   (project=cwd 루트, user=~/.claude)만 코호트 차원 키로 쓰고, Tier2
//!   (`@path` 참조)는 "존재했던 파일"로만 기록한다.
//! - **신선도 가드**: 마지막 관측이 FRESH_WINDOW_MIN보다 오래된 세션에는
//!   기록하지 않는다 — 죽은 세션에 오늘의 지시문을 붙이는 오염 방지.
use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};
use std::path::{Path, PathBuf};

const FRESH_WINDOW_MIN: i64 = 10;
const MAX_IMPORTS: usize = 20;
const MAX_FILE_BYTES: u64 = 1_000_000;
/// Tier3 트리 스캔 상한 — 깊이·방문 디렉토리 수(B-13). 존재 기록 전용이라
/// 놓침은 무해하고, 폭주만 막으면 된다.
const TREE_MAX_DEPTH: usize = 4;
const TREE_MAX_DIRS: usize = 400;
const TREE_SKIP: [&str; 6] = ["node_modules", "target", ".git", "dist", ".venv", "vendor"];

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

/// `@path` 참조 후보 추출 — 공백 경계 토큰만, 문서화된 import 문법의 보수적
/// 부분집합. 존재하는 파일만 Tier2로 기록되므로 오탐은 무해하다(존재 기록일 뿐).
fn import_candidates(content: &str) -> Vec<String> {
    content
        .split_whitespace()
        .filter_map(|tok| {
            let t = tok.strip_prefix('@')?;
            let t = t.trim_end_matches([',', '.', ';', ':', ')', ']', '"', '\'']);
            if t.is_empty() || t.contains('@') {
                return None;
            }
            if !t
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "_./~-".contains(c))
            {
                return None;
            }
            Some(t.to_string())
        })
        .take(MAX_IMPORTS)
        .collect()
}

/// 파일 하나를 스냅샷+관측으로 기록. 새 관측이면 (true, content)를 준다.
async fn record(
    pool: &SqlitePool,
    session_id: &str,
    source: &str,
    path: &Path,
) -> Result<(bool, Option<String>)> {
    let Ok(meta) = std::fs::metadata(path) else {
        return Ok((false, None));
    };
    if !meta.is_file() || meta.len() > MAX_FILE_BYTES {
        return Ok((false, None));
    }
    let Ok(content) = std::fs::read_to_string(path) else {
        return Ok((false, None));
    };
    let sha = sha256_hex(content.as_bytes());
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT OR IGNORE INTO instruction_snapshot (content_sha256, content, first_observed_at)
         VALUES (?, ?, ?)",
    )
    .bind(&sha)
    .bind(&content)
    .bind(&now)
    .execute(pool)
    .await?;
    let obs_id = format!(
        "io_{}",
        &sha256_hex(format!("{session_id}|{source}|{}|{sha}", path.display()).as_bytes())[..32]
    );
    let res = sqlx::query(
        "INSERT OR IGNORE INTO instruction_observation
             (observation_id, session_id, source, path, content_sha256, observed_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&obs_id)
    .bind(session_id)
    .bind(source)
    .bind(path.to_string_lossy().as_ref())
    .bind(&sha)
    .bind(&now)
    .execute(pool)
    .await?;
    Ok((res.rows_affected() > 0, Some(content)))
}

/// 세션의 지시문을 지금 관측한다. 반환 = 새로 기록된 관측 행 수.
/// `home_override`는 테스트용(기본 `dirs::home_dir()`).
pub async fn observe_session_instructions(
    pool: &SqlitePool,
    session_id: &str,
    home_override: Option<&Path>,
) -> Result<usize> {
    // 신선도 가드 — 최근 활동이 있는 세션만.
    let last: Option<String> =
        sqlx::query("SELECT MAX(observed_at) AS last FROM observed_event WHERE session_id = ?")
            .bind(session_id)
            .fetch_one(pool)
            .await?
            .get("last");
    let Some(last) = last else { return Ok(0) };
    let Ok(last_ts) = DateTime::parse_from_rfc3339(&last) else {
        return Ok(0);
    };
    if Utc::now() - last_ts.with_timezone(&Utc) > Duration::minutes(FRESH_WINDOW_MIN) {
        return Ok(0);
    }

    // 세션 루트 = 최초 이벤트의 cwd(launch dir). 레코드 cwd는 Bash `cd`로
    // 드리프트한다(실측 2026-07-04: 한 세션에 distinct cwd 4개) — 드리프트
    // cwd를 project 루트로 취급하면 하위 CLAUDE.md가 코호트 키('project')로
    // 오기록되므로 기준은 하나다.
    let launch_cwd: Option<String> = sqlx::query(
        "SELECT cwd FROM observed_event WHERE session_id = ? AND cwd IS NOT NULL
         ORDER BY observed_at ASC, event_id ASC LIMIT 1",
    )
    .bind(session_id)
    .fetch_optional(pool)
    .await?
    .map(|r| r.get("cwd"));

    let mut recorded = 0usize;
    let mut tier1_contents: Vec<(PathBuf, String)> = Vec::new();

    if let Some(cwd) = &launch_cwd {
        let p = Path::new(cwd).join("CLAUDE.md");
        let (new, content) = record(pool, session_id, "project", &p).await?;
        if new {
            recorded += 1;
        }
        if let Some(c) = content {
            tier1_contents.push((p, c));
        }
    }
    let home = home_override.map(Path::to_path_buf).or_else(dirs::home_dir);
    if let Some(h) = home {
        let p = h.join(".claude").join("CLAUDE.md");
        let (new, content) = record(pool, session_id, "user", &p).await?;
        if new {
            recorded += 1;
        }
        if let Some(c) = content {
            tier1_contents.push((p, c));
        }
    }

    // Tier3 — launch cwd 트리의 하위 CLAUDE.md(존재 기록만, 로드 무주장. B-13).
    if let Some(cwd) = &launch_cwd {
        for p in tree_claude_mds(Path::new(cwd)) {
            let (new, _) = record(pool, session_id, "tree", &p).await?;
            if new {
                recorded += 1;
            }
        }
    }

    // Tier2 — @path 참조(1단계, 존재 기록만).
    for (origin, content) in &tier1_contents {
        let base = origin.parent().unwrap_or(Path::new("."));
        for cand in import_candidates(content) {
            let p = if let Some(rest) = cand.strip_prefix("~/") {
                match dirs::home_dir() {
                    Some(h) => h.join(rest),
                    None => continue,
                }
            } else {
                base.join(&cand)
            };
            let (new, _) = record(pool, session_id, "import", &p).await?;
            if new {
                recorded += 1;
            }
        }
    }
    Ok(recorded)
}

/// cwd 하위(루트 제외)의 CLAUDE.md 나열 — 깊이/방문 상한, 무거운 디렉토리 제외.
fn tree_claude_mds(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack: Vec<(PathBuf, usize)> = vec![(root.to_path_buf(), 0)];
    let mut visited = 0usize;
    while let Some((dir, depth)) = stack.pop() {
        if depth > TREE_MAX_DEPTH || visited >= TREE_MAX_DIRS {
            continue;
        }
        visited += 1;
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            let name = e.file_name();
            let name = name.to_string_lossy();
            if p.is_dir() {
                if name.starts_with('.') || TREE_SKIP.contains(&name.as_ref()) {
                    continue;
                }
                stack.push((p, depth + 1));
            } else if name == "CLAUDE.md" && depth > 0 {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}
