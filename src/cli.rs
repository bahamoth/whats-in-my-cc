use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "witmcc", version, about = "What's in My Claude Code — slice-1")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    /// Path to the SQLite database file.
    #[arg(
        long,
        global = true,
        default_value = ".witmcc.sqlite",
        env = "WITMCC_DB"
    )]
    pub db_path: PathBuf,

    /// Log output format.
    #[arg(long, global = true, value_enum, default_value_t = LogFormat::Pretty)]
    pub log_format: LogFormat,

    /// Verbose logging (equivalent to RUST_LOG=debug).
    #[arg(short, long, global = true)]
    pub verbose: bool,
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
        /// witmcc server to probe. Defaults to WITMCC_SERVER or http://127.0.0.1:7878.
        #[arg(long, env = "WITMCC_SERVER", default_value = "http://127.0.0.1:7878")]
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
        /// Watch a directory for file/git changes (slice-5). If the path
        /// contains a `.git/` directory, a git poller also starts.
        #[arg(long)]
        watch: Option<PathBuf>,
        /// Polling interval for new git commits (seconds). Minimum 1.
        #[arg(long, default_value_t = 5)]
        git_poll_secs: u64,
        /// Test-only: auto-shutdown after N ms (used by smoke + cli tests).
        #[arg(long)]
        shutdown_after_ms: Option<u64>,
        /// Disable the transcript live tail (slice-7). Use this if you only
        /// want the OTel / hook receivers and prefer to backfill transcripts
        /// later with `witmcc ingest --all`.
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
    },
}
