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

    /// Path to the SQLite database file.
    #[arg(long, global = true, default_value = ".wimcc.sqlite", env = "WIMCC_DB")]
    pub db_path: PathBuf,

    /// Log output format.
    #[arg(long, global = true, value_enum, default_value_t = LogFormat::Pretty)]
    pub log_format: LogFormat,

    /// Verbose logging (equivalent to RUST_LOG=debug).
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Directory for rotating serve logs. Default: same directory as --db-path
    /// (CWD when the default `.wimcc.sqlite` is used). Only `serve` writes files.
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
    },
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
}
