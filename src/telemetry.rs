use std::path::{Path, PathBuf};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use crate::cli::LogFormat;

/// Resolve the directory for rotating serve log files.
///
/// Priority: an explicit `--log-dir`/`WIMCC_LOG_DIR` override → the parent
/// directory of `db_path` (log sits next to the DB) → CWD (`.`). The default
/// db path `.wimcc.sqlite` is a bare filename whose parent is the empty path,
/// which normalizes to `.` so "no settings" logs into the process CWD.
pub fn resolve_log_dir(db_path: &Path, override_dir: Option<&Path>) -> PathBuf {
    if let Some(dir) = override_dir {
        return dir.to_path_buf();
    }
    match db_path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

/// Build a daily-rotating file appender that writes `wimcc.YYYY-MM-DD.log` into
/// `dir`, keeping at most `keep_days` files (older ones are pruned on rotation).
/// The caller is responsible for ensuring `dir` exists.
pub fn file_appender(dir: &Path, keep_days: u16) -> RollingFileAppender {
    RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("wimcc")
        .filename_suffix("log")
        .max_log_files(keep_days as usize)
        .build(dir)
        .expect("build rolling file appender")
}

pub fn init(format: &LogFormat, verbose: bool) {
    let default_level = if verbose { "debug" } else { "info" };
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(format!("wimcc={default_level},sqlx=warn,axum=info")));

    let reg = tracing_subscriber::registry().with(filter);
    match format {
        LogFormat::Pretty => reg.with(fmt::layer().with_target(false)).init(),
        LogFormat::Json => reg.with(fmt::layer().json().with_target(false)).init(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::{Path, PathBuf};

    #[test]
    fn resolve_log_dir_defaults_to_db_parent() {
        // Bare filename (the default `.wimcc.sqlite`) has an empty parent → CWD.
        assert_eq!(
            resolve_log_dir(Path::new(".wimcc.sqlite"), None),
            PathBuf::from(".")
        );
        // A db inside a directory → that directory (log sits next to the DB).
        assert_eq!(
            resolve_log_dir(Path::new("/data/x.sqlite"), None),
            PathBuf::from("/data")
        );
        // An explicit override wins over the db-parent derivation.
        assert_eq!(
            resolve_log_dir(Path::new("/data/x.sqlite"), Some(Path::new("/logs"))),
            PathBuf::from("/logs")
        );
    }

    #[test]
    fn file_appender_writes_dated_wimcc_log() {
        let dir = tempfile::tempdir().expect("tempdir");
        {
            let mut app = file_appender(dir.path(), 7);
            writeln!(app, "hello from wimcc").expect("write");
            app.flush().expect("flush");
        }
        let names: Vec<String> = std::fs::read_dir(dir.path())
            .expect("read_dir")
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(
            names
                .iter()
                .any(|n| n.starts_with("wimcc") && n.ends_with(".log")),
            "expected a wimcc*.log file, got: {names:?}"
        );
    }
}
