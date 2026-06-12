//! SessionStart 수신 시각의 instruction 파일(CLAUDE.md) 스냅샷.
//!
//! 자기개선 루프의 독립변수(세션이 어떤 instruction 아래에서 돌았는가) 관측.
//! transcript에는 CLAUDE.md가 기록되지 않으므로(2026-06-12 실측: 4개 프로젝트
//! 12 transcript 전수에서 주입 블록 0건) hook 수신 "그 시점"에 서버가 후보
//! 파일을 해시한다 — 관측이지 설정 변경이 아니다(non-goal 비침해). 내용은
//! 저장하지 않는다(경로+sha256+크기만); 변경 감지는 해시 비교로, 내용 복원은
//! git의 몫이다.
//!
//! 후보 = cwd→파일시스템 루트 조상 디렉터리의 `CLAUDE.md` + `home/.claude/CLAUDE.md`.
//! Claude Code의 정확한 메모리 병합 의미론을 주장하지 않는다 — memory docs
//! (https://code.claude.com/docs/en/memory)의 파일 위치 규칙과 실 세션 관측
//! (2026-06-12, 표본 1: 조상 `/Users/<user>/CLAUDE.md` + 프로젝트 CLAUDE.md가
//! 컨텍스트에 주입됨)에 기반한 "존재 후보의 관측"이다. 읽기 실패는 조용히
//! 건너뛴다(OBS-3 degrade — 수집 실패가 실행을 깨지 않는다).

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::Path;

/// 관측된 instruction 파일 하나. session_start observed payload의
/// `/captured/claude_md[]`에 그대로 직렬화되고, fingerprint가 재사용한다.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct InstructionFile {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
}

/// cwd의 조상 경로 전체 + `home/.claude` 에서 존재하는 CLAUDE.md를 해시한다.
pub fn snapshot(cwd: &Path, home: Option<&Path>) -> Vec<InstructionFile> {
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    let mut dir = Some(cwd);
    while let Some(d) = dir {
        candidates.push(d.join("CLAUDE.md"));
        dir = d.parent();
    }
    if let Some(h) = home {
        candidates.push(h.join(".claude").join("CLAUDE.md"));
    }
    let mut out = Vec::new();
    for p in candidates {
        let Ok(content) = std::fs::read(&p) else {
            continue;
        };
        out.push(InstructionFile {
            path: p.to_string_lossy().into_owned(),
            sha256: hex::encode(Sha256::digest(&content)),
            bytes: content.len() as u64,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// std::env::temp_dir() 아래 고유 디렉터리 (tempfile dev-dep 불필요).
    fn scratch(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir()
            .join("wimcc_snap_test")
            .join(format!("{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn sha_hex(s: &str) -> String {
        hex::encode(Sha256::digest(s.as_bytes()))
    }

    #[test]
    fn collects_ancestor_and_home_claude_md() {
        let root = scratch("collect");
        let cwd = root.join("a").join("b");
        fs::create_dir_all(&cwd).unwrap();
        fs::write(root.join("CLAUDE.md"), "root rules").unwrap();
        fs::write(cwd.join("CLAUDE.md"), "project rules").unwrap();
        let home = root.join("home");
        fs::create_dir_all(home.join(".claude")).unwrap();
        fs::write(home.join(".claude").join("CLAUDE.md"), "user rules").unwrap();

        let got = snapshot(&cwd, Some(&home));

        // 조상 walk가 scratch 밖(/tmp, /)으로도 올라가므로 정확한 길이 대신
        // 기대 엔트리 포함 + 중복 부재를 단언한다.
        let find = |p: &std::path::Path| {
            got.iter()
                .find(|f| f.path == p.to_string_lossy())
                .unwrap_or_else(|| panic!("missing {}", p.display()))
                .clone()
        };
        let proj = find(&cwd.join("CLAUDE.md"));
        assert_eq!(proj.sha256, sha_hex("project rules"));
        assert_eq!(proj.bytes, "project rules".len() as u64);
        let root_f = find(&root.join("CLAUDE.md"));
        assert_eq!(root_f.sha256, sha_hex("root rules"));
        let user = find(&home.join(".claude").join("CLAUDE.md"));
        assert_eq!(user.sha256, sha_hex("user rules"));
        let mut paths: Vec<&str> = got.iter().map(|f| f.path.as_str()).collect();
        paths.sort_unstable();
        paths.dedup();
        assert_eq!(paths.len(), got.len(), "duplicate paths in snapshot");
    }

    #[test]
    fn missing_files_are_skipped_silently() {
        let root = scratch("missing");
        let cwd = root.join("x");
        fs::create_dir_all(&cwd).unwrap();
        let got = snapshot(&cwd, None);
        assert!(
            got.iter()
                .all(|f| !f.path.starts_with(root.to_string_lossy().as_ref())),
            "no CLAUDE.md under scratch must yield no scratch entries"
        );
    }
}
