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
    },
}
