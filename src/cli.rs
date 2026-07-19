use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// Whether to require bearer-token authentication on Pull API + MCP.
/// Default is `Off` for single-user local dev — `On` to enforce the slice-19
/// bearer scheme (token in `~/Library/Application Support/wimcc/token` on
/// macOS, `~/.config/wimcc/token` on Linux). DEV-S19-08.
#[derive(Debug, Clone, PartialEq, Eq, clap::ValueEnum)]
pub enum AuthMode {
    Off,
    On,
}

#[derive(Debug, Parser)]
#[command(
    name = "wimcc",
    version,
    about = "What's in My Claude Code — local execution inspection"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    /// Path to the SQLite database file. Default: `<data_dir>/wimcc/wimcc.sqlite`
    /// (macOS `~/Library/Application Support`, Linux `~/.local/share`); when the
    /// current directory has a legacy `./.wimcc.sqlite`, that file is used
    /// instead (logged at startup). Resolution: `paths::resolve_db_path`.
    #[arg(long, global = true, env = "WIMCC_DB")]
    pub db_path: Option<PathBuf>,

    /// Log output format.
    #[arg(long, global = true, value_enum, default_value_t = LogFormat::Pretty)]
    pub log_format: LogFormat,

    /// Verbose logging (equivalent to RUST_LOG=debug).
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Directory for rotating serve logs. Default: parent directory of the
    /// resolved --db-path (CWD for a legacy `./.wimcc.sqlite`, the wimcc data
    /// directory for the default). Only `serve` writes files.
    #[arg(long, global = true, env = "WIMCC_LOG_DIR")]
    pub log_dir: Option<PathBuf>,

    /// How many daily serve log files to keep before pruning. Default: 7.
    #[arg(
        long,
        global = true,
        default_value_t = 7,
        value_parser = clap::value_parser!(u16).range(1..=365),
        env = "WIMCC_LOG_RETENTION_DAYS"
    )]
    pub log_retention_days: u16,
}

#[derive(Debug, Clone, clap::ValueEnum)]
pub enum LogFormat {
    Pretty,
    Json,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Apply migrations and prepare the database.
    InitDb,
    /// Compact the database file: convert to auto_vacuum=INCREMENTAL and run
    /// a full VACUUM so freed pages return to the filesystem. VACUUM takes an
    /// exclusive lock — run while serve is stopped.
    Vacuum,
    /// Scan transcript JSONL files and insert raw + observed events.
    Ingest {
        /// A specific file or directory to ingest.
        #[arg(long, conflicts_with = "all")]
        path: Option<PathBuf>,
        /// Auto-discover ~/.claude/projects/**/*.jsonl
        #[arg(long, conflicts_with = "path")]
        all: bool,
    },
    /// Diagnose collector wiring — read-only env / hook settings / server probe.
    /// No file mutation (CLAUDE.md non-goal).
    Doctor {
        /// Emit structured JSON instead of the pretty table.
        #[arg(long)]
        json: bool,
        /// wimcc server to probe. Defaults to WIMCC_SERVER or http://127.0.0.1:7878.
        #[arg(long, env = "WIMCC_SERVER", default_value = "http://127.0.0.1:7878")]
        server: String,
        /// Project root to walk for `.claude/settings.json` and `.claude/settings.local.json`.
        /// Defaults to CWD; slice-7 settings hierarchy.
        #[arg(long)]
        project: Option<PathBuf>,
    },
    /// Start the read-only Pull API HTTP server.
    Serve {
        #[arg(long, default_value = "127.0.0.1")]
        bind: std::net::IpAddr,
        #[arg(long, default_value_t = 7878)]
        port: u16,
        /// Apply pending migrations on startup instead of refusing.
        #[arg(long)]
        auto_migrate: bool,
        /// Test-only: auto-shutdown after N ms (used by smoke + cli tests).
        #[arg(long)]
        shutdown_after_ms: Option<u64>,
        /// Disable the transcript live tail (slice-7). Use this if you only
        /// want the OTel / hook receivers and prefer to backfill transcripts
        /// later with `wimcc ingest --all`.
        #[arg(long)]
        no_watch_transcripts: bool,
        /// Override the transcripts root that the live tail watches.
        /// Default: `~/.claude/projects` (autodetected).
        #[arg(long)]
        transcripts_root: Option<PathBuf>,
        /// SSE keep-alive comment interval in seconds. Sent as `:keepalive\n\n`
        /// on the WebUI live stream so middle proxies do not idle-time-out.
        #[arg(long, default_value_t = 30, value_parser = clap::value_parser!(u64).range(5..=120))]
        sse_keepalive_secs: u64,
        /// Capacity of the in-process broadcast channel that ingest writers
        /// emit into and the SSE handler subscribes to.
        #[arg(long, default_value_t = 512, value_parser = clap::value_parser!(u64).range(64..=8192))]
        sse_channel_capacity: u64,
        /// Slice-19: Print the current token to stderr and exit. Does not start the server.
        #[arg(long, conflicts_with = "rotate_token")]
        print_token: bool,
        /// Slice-19: Generate a new token, overwrite the token file, print it to stderr and exit.
        /// Existing connections receive 401 on their next request.
        #[arg(long, conflicts_with = "print_token")]
        rotate_token: bool,
        /// Slice-19: Retention profile. Default: "default" (raw 60d,
        /// normalized+signal 180d, audit 90d) — a 6-hourly sweep over all data
        /// classes. "none" keeps everything forever (explicit archiving opt-out);
        /// "strict" (raw 7d, others 30d) is more aggressive.
        #[arg(long, default_value = "default", value_parser = ["none", "default", "strict"])]
        retention_profile: String,
        /// Whether to require bearer-token auth on /v1 + /mcp. Default: off (single-user dev).
        /// `on` activates slice-19 token middleware. DEV-S19-08.
        #[arg(long, value_enum, default_value_t = AuthMode::Off)]
        auth: AuthMode,
        /// Daily new-release check against GitHub Releases metadata (the only
        /// outbound call). With "off" wimcc makes no outbound calls at all.
        // 스펙 2026-07-17 §4.
        #[arg(long, default_value = "on", value_parser = ["on", "off"], env = "WIMCC_UPDATE_CHECK")]
        update_check: String,
        /// Download a newer release in the background when the update check
        /// observes one (shell installs only; package-manager installs are
        /// guided to their manager). The swapped binary takes effect on the
        /// next restart — a running serve is never restarted automatically.
        // 2026-07-19 사용자 결정: 다운로드까지 자동, 재시작은 수동(라이브
        // 세션 관측 중단 방지). 채널 판별은 self_update::decide 재사용.
        #[arg(long, default_value = "off", value_parser = ["on", "off"], env = "WIMCC_AUTO_UPDATE")]
        auto_update: String,
    },
    /// Replace the binary with the latest release. Never restarts a running serve.
    SelfUpdate {
        /// Check for a newer release without replacing anything.
        #[arg(long)]
        check: bool,
    },
    /// Register/unregister serve as an OS user service (macOS launchd, Linux systemd --user).
    Service {
        #[command(subcommand)]
        action: ServiceAction,
    },
}

#[derive(Debug, Subcommand)]
pub enum ServiceAction {
    /// Register serve to start on login. The global --db-path is recorded as an absolute path.
    Install {
        #[arg(long, default_value = "127.0.0.1")]
        bind: std::net::IpAddr,
        #[arg(long, default_value_t = 7878)]
        port: u16,
        /// Services boot unattended and cannot answer a migration prompt — default on.
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        auto_migrate: bool,
    },
    Uninstall,
    Restart,
    Status,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-07-08 사용자 결정: retention은 스펙상 "제품 핵심 요구사항"인데
    /// 기본이 none이면 무한 증가가 기본이 된다 — 이름대로 `default` 프로파일을
    /// 실제 기본값으로 승격한다. none은 명시적 아카이빙 opt-out으로 남는다.
    #[test]
    fn serve_defaults_to_default_retention_profile() {
        let cli = Cli::try_parse_from(["wimcc", "serve"]).expect("serve parses with defaults");
        match cli.command {
            Command::Serve {
                retention_profile, ..
            } => assert_eq!(retention_profile, "default"),
            other => panic!("expected Serve, got {other:?}"),
        }
    }

    /// 2026-07-10: 롤링 파일 로거 — 위치·보관수는 전역 옵션. 기본값 확인.
    #[test]
    fn log_dir_and_retention_days_defaults() {
        let cli = Cli::try_parse_from(["wimcc", "serve"]).expect("serve parses");
        assert_eq!(cli.log_dir, None);
        assert_eq!(cli.log_retention_days, 7);
    }

    #[test]
    fn log_dir_and_retention_days_from_args() {
        let cli = Cli::try_parse_from([
            "wimcc",
            "--log-dir",
            "/logs",
            "--log-retention-days",
            "14",
            "serve",
        ])
        .expect("parses global log flags");
        assert_eq!(cli.log_dir.as_deref(), Some(std::path::Path::new("/logs")));
        assert_eq!(cli.log_retention_days, 14);
    }

    #[test]
    fn log_retention_days_rejects_out_of_range() {
        assert!(Cli::try_parse_from(["wimcc", "--log-retention-days", "0", "serve"]).is_err());
        assert!(Cli::try_parse_from(["wimcc", "--log-retention-days", "366", "serve"]).is_err());
    }

    #[test]
    fn serve_update_check_defaults_on() {
        let cli = Cli::try_parse_from(["wimcc", "serve"]).expect("parses");
        match cli.command {
            Command::Serve { update_check, .. } => assert_eq!(update_check, "on"),
            other => panic!("expected Serve, got {other:?}"),
        }
    }

    /// 2026-07-19 사용자 결정 — auto-update는 명시적 opt-in(기본 off):
    /// 바이너리 교체는 부수효과가 커서 관측 없이 켜지지 않는다.
    #[test]
    fn serve_auto_update_defaults_off() {
        let cli = Cli::try_parse_from(["wimcc", "serve"]).expect("parses");
        match cli.command {
            Command::Serve { auto_update, .. } => assert_eq!(auto_update, "off"),
            other => panic!("expected Serve, got {other:?}"),
        }
    }

    #[test]
    fn self_update_parses_with_check_flag() {
        let cli = Cli::try_parse_from(["wimcc", "self-update", "--check"]).expect("parses");
        match cli.command {
            Command::SelfUpdate { check } => assert!(check),
            other => panic!("expected SelfUpdate, got {other:?}"),
        }
    }

    /// 2026-07-18: 기본값을 clap에서 빼고 `paths::resolve_db_path`로 옮겼다 —
    /// CWD 상대 default_value가 있으면 legacy 폴백 판정이 불가능하다.
    #[test]
    fn db_path_has_no_eager_default() {
        let cli = Cli::try_parse_from(["wimcc", "serve"]).expect("parses");
        assert!(cli.db_path.is_none());
    }

    #[test]
    fn db_path_flag_is_explicit() {
        let cli =
            Cli::try_parse_from(["wimcc", "--db-path", "/x/y.sqlite", "serve"]).expect("parses");
        assert_eq!(
            cli.db_path.as_deref(),
            Some(std::path::Path::new("/x/y.sqlite"))
        );
    }

    #[test]
    fn service_install_defaults() {
        let cli = Cli::try_parse_from(["wimcc", "service", "install"]).expect("parses");
        match cli.command {
            Command::Service {
                action:
                    ServiceAction::Install {
                        port, auto_migrate, ..
                    },
            } => {
                assert_eq!(port, 7878);
                assert!(auto_migrate, "무인 기동이므로 auto-migrate 기본 on");
            }
            other => panic!("expected Service Install, got {other:?}"),
        }
    }
}
