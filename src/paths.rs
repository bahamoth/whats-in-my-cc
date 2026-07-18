use std::path::{Path, PathBuf};

pub fn default_transcripts_root() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".claude").join("projects"))
}

/// 구(舊) 기본값 — CWD 상대 파일명. 실행 위치마다 DB가 흩어지는 문제로
/// 2026-07-18 기본값을 플랫폼 데이터 디렉터리로 바꾸면서 폴백으로 강등됐다.
pub const LEGACY_DB_FILENAME: &str = ".wimcc.sqlite";
/// 데이터 디렉터리 기본 파일명(숨김 아님 — 전용 디렉터리 안이라 dot 불필요).
pub const DB_FILENAME: &str = "wimcc.sqlite";

/// `--db-path` 해석 결과의 출처. serve 시작 로그·doctor 리포트에 표기된다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbPathSource {
    /// `--db-path` 플래그 또는 `WIMCC_DB` env.
    Explicit,
    /// CWD에 legacy `.wimcc.sqlite`가 존재해 그것을 사용(구버전 연속성).
    LegacyCwd,
    /// 플랫폼 데이터 디렉터리 기본값(`<data_dir>/wimcc/wimcc.sqlite`).
    DataDir,
}

impl DbPathSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            DbPathSource::Explicit => "--db-path/WIMCC_DB",
            DbPathSource::LegacyCwd => "legacy-cwd",
            DbPathSource::DataDir => "data-dir",
        }
    }
}

/// DB 경로 해석(순수 함수 — cwd·data_dir 주입으로 테스트 가능).
/// 우선순위: ① 명시(`--db-path`/`WIMCC_DB`) ② CWD의 legacy `.wimcc.sqlite`
/// 존재 시 그 파일 ③ `<data_dir>/wimcc/wimcc.sqlite`. data_dir을 알 수 없는
/// 환경(홈 없음)은 CWD legacy 경로로 폴백한다.
pub fn resolve_db_path_in(
    explicit: Option<PathBuf>,
    cwd: &Path,
    data_dir: Option<&Path>,
) -> (PathBuf, DbPathSource) {
    if let Some(p) = explicit {
        return (p, DbPathSource::Explicit);
    }
    let legacy = cwd.join(LEGACY_DB_FILENAME);
    if legacy.exists() {
        return (legacy, DbPathSource::LegacyCwd);
    }
    match data_dir {
        Some(d) => (d.join("wimcc").join(DB_FILENAME), DbPathSource::DataDir),
        None => (legacy, DbPathSource::LegacyCwd),
    }
}

/// 실환경 wrapper — CWD와 `dirs::data_dir()`를 물려 해석한다.
pub fn resolve_db_path(explicit: Option<PathBuf>) -> (PathBuf, DbPathSource) {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    resolve_db_path_in(explicit, &cwd, dirs::data_dir().as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn explicit_path_wins_even_when_legacy_db_exists() {
        let cwd = tempfile::tempdir().unwrap();
        std::fs::write(cwd.path().join(LEGACY_DB_FILENAME), b"").unwrap();
        let data = tempfile::tempdir().unwrap();
        let (path, source) = resolve_db_path_in(
            Some(PathBuf::from("/explicit/custom.sqlite")),
            cwd.path(),
            Some(data.path()),
        );
        assert_eq!(path, Path::new("/explicit/custom.sqlite"));
        assert_eq!(source, DbPathSource::Explicit);
    }

    #[test]
    fn legacy_cwd_db_is_detected_and_used() {
        let cwd = tempfile::tempdir().unwrap();
        std::fs::write(cwd.path().join(LEGACY_DB_FILENAME), b"").unwrap();
        let data = tempfile::tempdir().unwrap();
        let (path, source) = resolve_db_path_in(None, cwd.path(), Some(data.path()));
        assert_eq!(path, cwd.path().join(LEGACY_DB_FILENAME));
        assert_eq!(source, DbPathSource::LegacyCwd);
    }

    #[test]
    fn default_is_platform_data_dir_when_no_legacy_db() {
        let cwd = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let (path, source) = resolve_db_path_in(None, cwd.path(), Some(data.path()));
        assert_eq!(path, data.path().join("wimcc").join(DB_FILENAME));
        assert_eq!(source, DbPathSource::DataDir);
    }

    #[test]
    fn missing_data_dir_falls_back_to_cwd_path() {
        let cwd = tempfile::tempdir().unwrap();
        let (path, source) = resolve_db_path_in(None, cwd.path(), None);
        assert_eq!(path, cwd.path().join(LEGACY_DB_FILENAME));
        assert_eq!(source, DbPathSource::LegacyCwd);
    }
}
