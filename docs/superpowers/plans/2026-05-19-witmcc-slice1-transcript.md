# Slice-1 Transcript Vertical Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Rust binary `witmcc` that ingests Claude Code transcript JSONL files from `~/.claude/projects/**/*.jsonl` into a local SQLite store, builds a deterministic-edge session graph, and serves a read-only Pull API on 127.0.0.1 — locking schema and module boundaries so the OTel/Hook/File-Git/UI/MCP/redaction slices land on top without rework.

**Architecture:** Single binary, single crate. Three subcommands (`init-db | ingest | serve`) compose a pipeline: `walkdir → tokio LinesStream → ParsedRecord → ObservedEvent + RawEvent → backfill turn_id → rebuild graph (hash-derived deterministic node/edge ids) → axum GET handlers with `{meta, data}` envelope`. Model types live in `src/model/` and stay pure; sqlx queries live in `src/db/` and target those types via `query_as!`.

**Tech Stack:** Rust 1.78 (edition 2021), tokio 1.40, axum 0.7, sqlx 0.8 (sqlite + macros + migrate + tokio-rustls), clap 4.5 derive, serde + serde_json, ulid 1.1 (monotonic generator), sha2 + hex (hash-derived ids), chrono 0.4, tracing + tracing-subscriber, walkdir 2, dirs 5, anyhow + thiserror; dev: insta, assert-json-diff, axum-test, tempfile, assert_cmd, pretty_assertions.

**Reference spec:** `docs/superpowers/specs/2026-05-19-witmcc-slice1-transcript-design.md` — read once before starting and refer back when a task references a section.

---

## File Structure (locked at plan-time)

| Path | Responsibility |
|---|---|
| `Cargo.toml`, `rust-toolchain.toml`, `.gitignore` | Project metadata, pinned toolchain (1.78), git-ignore `target/`, `.witmcc.sqlite*`, `payloads/` |
| `migrations/20260519120000_0001_init.sql` | All slice-1 tables and indexes |
| `src/main.rs` | clap `Cli` parse + dispatch to subcommand handlers |
| `src/cli.rs` | `Cli` / `Command` enums for clap |
| `src/telemetry.rs` | `init_tracing()` |
| `src/error.rs` | `WitmccError` (thiserror) + `Result<T>` alias |
| `src/ids.rs` | `new_event_id()` (ULID), `derive_node_id()`, `derive_edge_id()` |
| `src/paths.rs` | `default_db_path()`, `default_transcripts_root()` |
| `src/model/mod.rs` + `raw.rs` + `observed.rs` + `graph.rs` + `meta.rs` | Pure data types, no I/O |
| `src/db/mod.rs` | `connect()` with WAL/FK/busy_timeout, `migrate()` |
| `src/db/repo_raw.rs` | insert + dedupe RawEvent |
| `src/db/repo_observed.rs` | insert ObservedEvent, session list/detail reads |
| `src/db/repo_graph.rs` | delete-then-insert nodes/edges, read by session |
| `src/db/repo_runs.rs` | ingest_run lifecycle |
| `src/ingest/mod.rs` | `ingest_path()` orchestrator |
| `src/ingest/discovery.rs` | `walkdir` JSONL candidates |
| `src/ingest/transcript.rs` | `stream_file()`, `ParsedRecord` enum, per-type structs |
| `src/ingest/mapping.rs` | `ParsedRecord` → `ObservedEvent`s + payload |
| `src/ingest/store.rs` | per-file txn + `backfill_turn_ids(session_id)` |
| `src/graph/mod.rs` + `build.rs` | `rebuild_session(session_id)` |
| `src/api/mod.rs` | `Router::new()…`, bind, host middleware wiring |
| `src/api/middleware.rs` | Host header allowlist |
| `src/api/routes.rs` | 4 handlers |
| `src/api/dto.rs` | response DTOs + envelope |
| `tests/fixtures/transcripts/*.jsonl` | Hand-built fixtures (minimal, dangling, sidechain, malformed, large) |
| `tests/cli.rs` | end-to-end smoke (assert_cmd) |
| `tests/determinism.rs` | re-run idempotency + node/edge PK stability |

---

## Task 1: Cargo bootstrap and toolchain

**Files:**
- Create: `Cargo.toml`
- Create: `rust-toolchain.toml`
- Create: `.gitignore`
- Create: `src/main.rs`
- Test: `cargo build` succeeds

- [ ] **Step 1: Write `rust-toolchain.toml`**

```toml
[toolchain]
channel = "1.78.0"
components = ["rustfmt", "clippy"]
```

- [ ] **Step 2: Write `Cargo.toml`**

```toml
[package]
name = "witmcc"
version = "0.1.0"
edition = "2021"
rust-version = "1.78"
description = "What's in My Claude Code — local execution inspection (slice-1: transcript)"

[dependencies]
tokio              = { version = "1.40", features = ["macros","rt-multi-thread","fs","io-util","signal"] }
axum               = { version = "0.7",  features = ["macros","json","tokio"] }
tower              = "0.5"
tower-http         = { version = "0.5",  features = ["trace"] }
sqlx               = { version = "0.8",  default-features = false, features = ["runtime-tokio-rustls","sqlite","macros","migrate","json","chrono"] }
serde              = { version = "1",    features = ["derive"] }
serde_json         = { version = "1",    features = ["raw_value","preserve_order"] }
clap               = { version = "4.5",  features = ["derive","env"] }
tracing            = "0.1"
tracing-subscriber = { version = "0.3",  features = ["env-filter","json"] }
ulid               = { version = "1.1",  features = ["serde"] }
chrono             = { version = "0.4",  features = ["serde"] }
anyhow             = "1"
thiserror          = "2"
dirs               = "5"
walkdir            = "2"
futures            = "0.3"
tokio-stream       = { version = "0.1", features = ["io-util"] }
sha2               = "0.10"
hex                = "0.4"
once_cell          = "1"

[dev-dependencies]
assert-json-diff   = "2"
insta              = { version = "1", features = ["json","redactions"] }
tempfile           = "3"
http-body-util     = "0.1"
axum-test          = "16"
pretty_assertions  = "1"
assert_cmd         = "2"

[profile.release]
lto = "thin"
```

- [ ] **Step 3: Write `.gitignore`**

```gitignore
/target
**/*.rs.bk
.witmcc.sqlite*
payloads/
.DS_Store
```

- [ ] **Step 4: Write minimal `src/main.rs`**

```rust
fn main() {
    println!("witmcc");
}
```

- [ ] **Step 5: Verify build**

Run: `cargo build`
Expected: compiles. If sqlx complains about `SQLX_OFFLINE`, ignore for now — no `query!` macros in scope yet.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml rust-toolchain.toml .gitignore src/main.rs
git commit -m "chore: cargo bootstrap (witmcc slice-1)"
```

---

## Task 2: clap CLI scaffold

**Files:**
- Create: `src/cli.rs`
- Modify: `src/main.rs`
- Test: `tests/cli_help.rs`

- [ ] **Step 1: Write failing test `tests/cli_help.rs`**

```rust
use assert_cmd::Command;

#[test]
fn help_shows_subcommands() {
    let assert = Command::cargo_bin("witmcc").unwrap().arg("--help").assert().success();
    let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    for sub in ["init-db", "ingest", "serve"] {
        assert!(out.contains(sub), "missing subcommand in help: {sub}\n{out}");
    }
}
```

- [ ] **Step 2: Run — expect failure**

Run: `cargo test --test cli_help`
Expected: FAIL (binary prints "witmcc", help has no subcommands).

- [ ] **Step 3: Write `src/cli.rs`**

```rust
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "witmcc", version, about = "What's in My Claude Code — slice-1")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    /// Path to the SQLite database file.
    #[arg(long, global = true, default_value = ".witmcc.sqlite", env = "WITMCC_DB")]
    pub db_path: PathBuf,

    /// Log output format.
    #[arg(long, global = true, value_enum, default_value_t = LogFormat::Pretty)]
    pub log_format: LogFormat,

    /// Verbose logging (equivalent to RUST_LOG=debug).
    #[arg(short, long, global = true)]
    pub verbose: bool,
}

#[derive(Debug, Clone, clap::ValueEnum)]
pub enum LogFormat { Pretty, Json }

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
        bind: String,
        #[arg(long, default_value_t = 7878)]
        port: u16,
        /// Apply pending migrations on startup instead of refusing.
        #[arg(long)]
        auto_migrate: bool,
    },
}
```

- [ ] **Step 4: Replace `src/main.rs`**

```rust
mod cli;

use clap::Parser;

fn main() {
    let _cli = cli::Cli::parse();
    // Subcommand handlers will be wired in later tasks.
    std::process::exit(0);
}
```

- [ ] **Step 5: Run — expect pass**

Run: `cargo test --test cli_help`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/cli.rs src/main.rs tests/cli_help.rs
git commit -m "feat(cli): scaffold init-db/ingest/serve subcommands"
```

---

## Task 3: telemetry + error modules

**Files:**
- Create: `src/telemetry.rs`
- Create: `src/error.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Write `src/error.rs`**

```rust
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WitmccError {
    #[error("io error at {path}: {source}")]
    Io { path: PathBuf, #[source] source: std::io::Error },

    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),

    #[error("migration error: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),

    #[error("invalid input: {0}")]
    Invalid(String),

    #[error("json parse error at {source_uri}:{line_no}: {message}")]
    ParseLine { source_uri: String, line_no: u64, message: String },

    #[error("not found: {0}")]
    NotFound(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, WitmccError>;
```

- [ ] **Step 2: Write `src/telemetry.rs`**

```rust
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use crate::cli::LogFormat;

pub fn init(format: &LogFormat, verbose: bool) {
    let default_level = if verbose { "debug" } else { "info" };
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(format!("witmcc={default_level},sqlx=warn,axum=info")));

    let reg = tracing_subscriber::registry().with(filter);
    match format {
        LogFormat::Pretty => reg.with(fmt::layer().with_target(false)).init(),
        LogFormat::Json => reg.with(fmt::layer().json().with_target(false)).init(),
    }
}
```

- [ ] **Step 3: Wire from `src/main.rs`**

```rust
mod cli;
mod error;
mod telemetry;

use clap::Parser;

fn main() -> error::Result<()> {
    let cli = cli::Cli::parse();
    telemetry::init(&cli.log_format, cli.verbose);
    tracing::info!(?cli.command, "witmcc starting");
    Ok(())
}
```

- [ ] **Step 4: Verify build + cli_help still passes**

Run: `cargo build && cargo test --test cli_help`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/error.rs src/telemetry.rs src/main.rs
git commit -m "feat(core): telemetry + WitmccError"
```

---

## Task 4: id derivation (ULID + hash-derived node/edge ids)

**Files:**
- Create: `src/ids.rs`
- Modify: `src/main.rs` (add `mod ids;`)

- [ ] **Step 1: Write failing tests in `src/ids.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_node_id_is_deterministic() {
        let a = derive_node_id("tool_call", &[("session_id","s1"),("tool_use_id","toolu_abc")]);
        let b = derive_node_id("tool_call", &[("tool_use_id","toolu_abc"),("session_id","s1")]);
        assert_eq!(a, b, "must be order-independent (sorted internally)");
        assert!(a.starts_with("nd_"));
        assert_eq!(a.len(), 3 + 24);
    }

    #[test]
    fn derive_node_id_differs_when_kind_differs() {
        let a = derive_node_id("tool_call",   &[("session_id","s1"),("tool_use_id","toolu_abc")]);
        let b = derive_node_id("tool_result", &[("session_id","s1"),("tool_use_id","toolu_abc")]);
        assert_ne!(a, b);
    }

    #[test]
    fn derive_edge_id_directional() {
        let n1 = derive_node_id("tool_call",   &[("session_id","s1"),("tool_use_id","t1")]);
        let n2 = derive_node_id("tool_result", &[("session_id","s1"),("tool_use_id","t1")]);
        let fwd = derive_edge_id(&n1, &n2, "tool_call_to_result");
        let back = derive_edge_id(&n2, &n1, "tool_call_to_result");
        assert_ne!(fwd, back);
        assert!(fwd.starts_with("eg_"));
    }

    #[test]
    fn event_ids_are_monotonic_in_one_thread() {
        let mut gen = MonotonicUlidGen::new();
        let a = gen.next();
        let b = gen.next();
        assert!(b > a, "ulid must be monotonic");
    }
}
```

- [ ] **Step 2: Write `src/ids.rs` body**

```rust
use sha2::{Digest, Sha256};
use ulid::Generator;

pub fn derive_node_id(kind: &str, keys: &[(&str, &str)]) -> String {
    let mut sorted: Vec<(&str, &str)> = keys.iter().copied().collect();
    sorted.sort_by(|a, b| a.0.cmp(b.0));
    let canonical = sorted.iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(";");
    let mut h = Sha256::new();
    h.update(kind.as_bytes());
    h.update(b"|");
    h.update(canonical.as_bytes());
    format!("nd_{}", hex::encode(&h.finalize()[..12]))
}

pub fn derive_edge_id(from_id: &str, to_id: &str, kind: &str) -> String {
    let mut h = Sha256::new();
    h.update(from_id.as_bytes());
    h.update(b">");
    h.update(to_id.as_bytes());
    h.update(b"#");
    h.update(kind.as_bytes());
    format!("eg_{}", hex::encode(&h.finalize()[..12]))
}

/// Single-task monotonic ULID generator. Must NOT be shared across tasks
/// without external synchronization — slice-1 keeps ingest single-task.
pub struct MonotonicUlidGen { inner: Generator }

impl MonotonicUlidGen {
    pub fn new() -> Self { Self { inner: Generator::new() } }
    pub fn next(&mut self) -> String {
        // unwrap acceptable: only fails on monotonic overflow within same ms (extremely unlikely).
        self.inner.generate().expect("ulid generator overflow").to_string()
    }
}

impl Default for MonotonicUlidGen { fn default() -> Self { Self::new() } }
```

- [ ] **Step 3: Run tests**

Modify `src/main.rs` to add `mod ids;`. Run: `cargo test ids::tests`
Expected: PASS for all four tests.

- [ ] **Step 4: Commit**

```bash
git add src/ids.rs src/main.rs
git commit -m "feat(ids): hash-derived node/edge ids + monotonic ULID generator"
```

---

## Task 5: Initial SQL migration

**Files:**
- Create: `migrations/20260519120000_0001_init.sql`

- [ ] **Step 1: Create the migration file with the exact schema from the spec**

```sql
-- 0001_init: slice-1 transcript schema
PRAGMA foreign_keys = ON;

CREATE TABLE ingest_run (
    run_id      TEXT PRIMARY KEY,
    started_at  TEXT NOT NULL,
    finished_at TEXT,
    status      TEXT NOT NULL,
    stats       TEXT
);

CREATE TABLE raw_event (
    raw_event_id        TEXT PRIMARY KEY,
    ingest_run_id       TEXT NOT NULL REFERENCES ingest_run(run_id),
    source_type         TEXT NOT NULL,
    source_uri          TEXT NOT NULL,
    source_line_no      INTEGER NOT NULL,
    source_byte_offset  INTEGER NOT NULL,
    payload_sha256      TEXT NOT NULL,
    payload             BLOB NOT NULL,
    parse_error         TEXT,
    captured_at         TEXT NOT NULL,
    UNIQUE(source_uri, source_line_no, payload_sha256)
);
CREATE INDEX idx_raw_event_source ON raw_event(source_uri, source_line_no);

CREATE TABLE observed_event (
    event_id                   TEXT PRIMARY KEY,
    raw_event_id               TEXT NOT NULL REFERENCES raw_event(raw_event_id),
    schema_version             TEXT NOT NULL,
    session_id                 TEXT NOT NULL,
    event_uuid                 TEXT,
    parent_uuid                TEXT,
    observed_at                TEXT NOT NULL,
    actor                      TEXT NOT NULL,
    kind                       TEXT NOT NULL,
    subkind                    TEXT,
    tool_use_id                TEXT,
    tool_name                  TEXT,
    request_id                 TEXT,
    message_id                 TEXT,
    turn_id                    TEXT,
    source_tool_assistant_uuid TEXT,
    source_tool_use_id         TEXT,
    is_sidechain               INTEGER NOT NULL DEFAULT 0,
    is_meta                    INTEGER NOT NULL DEFAULT 0,
    cwd                        TEXT,
    git_branch                 TEXT,
    user_type                  TEXT,
    entrypoint                 TEXT,
    cc_version                 TEXT,
    payload                    TEXT NOT NULL,
    trace_id                   TEXT,
    span_id                    TEXT,
    parent_span_id             TEXT,
    latency_ms                 INTEGER,
    redaction_state            TEXT,
    parser_version             TEXT NOT NULL
);
CREATE INDEX idx_obs_session_time ON observed_event(session_id, observed_at);
CREATE INDEX idx_obs_tool_use_id  ON observed_event(tool_use_id) WHERE tool_use_id IS NOT NULL;
CREATE INDEX idx_obs_event_uuid   ON observed_event(event_uuid)  WHERE event_uuid  IS NOT NULL;
CREATE INDEX idx_obs_parent_uuid  ON observed_event(parent_uuid) WHERE parent_uuid IS NOT NULL;
CREATE INDEX idx_obs_turn_id      ON observed_event(session_id, turn_id);

CREATE TABLE graph_node (
    node_id          TEXT PRIMARY KEY,
    schema_version   TEXT NOT NULL,
    session_id       TEXT NOT NULL,
    node_kind        TEXT NOT NULL,
    started_at       TEXT NOT NULL,
    ended_at         TEXT,
    merge_keys       TEXT NOT NULL,
    source_event_ids TEXT NOT NULL,
    source_uris      TEXT NOT NULL,
    payload          TEXT NOT NULL
);
CREATE INDEX idx_graph_node_session ON graph_node(session_id, started_at);
CREATE INDEX idx_graph_node_kind    ON graph_node(session_id, node_kind);

CREATE TABLE graph_edge (
    edge_id        TEXT PRIMARY KEY,
    schema_version TEXT NOT NULL,
    session_id     TEXT NOT NULL,
    from_node_id   TEXT NOT NULL REFERENCES graph_node(node_id),
    to_node_id     TEXT NOT NULL REFERENCES graph_node(node_id),
    edge_kind      TEXT NOT NULL,
    origin         TEXT NOT NULL DEFAULT 'deterministic',
    attributes     TEXT NOT NULL DEFAULT '{}'
);
CREATE INDEX idx_graph_edge_session ON graph_edge(session_id, edge_kind);
CREATE INDEX idx_graph_edge_from    ON graph_edge(from_node_id);
CREATE INDEX idx_graph_edge_to      ON graph_edge(to_node_id);
```

- [ ] **Step 2: Commit the migration without wiring it (Task 6 wires it)**

```bash
git add migrations/20260519120000_0001_init.sql
git commit -m "feat(db): initial migration — slice-1 schema"
```

---

## Task 6: DB module — pool + migrate + `init-db`

**Files:**
- Create: `src/db/mod.rs`
- Modify: `src/main.rs`
- Test: `tests/db_init.rs`

- [ ] **Step 1: Write failing test `tests/db_init.rs`**

```rust
use sqlx::sqlite::SqlitePoolOptions;

#[tokio::test]
async fn migrate_creates_expected_tables() {
    let url = "sqlite::memory:";
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(url).await.unwrap();
    witmcc::db::migrate(&pool).await.unwrap();
    for t in ["ingest_run","raw_event","observed_event","graph_node","graph_edge"] {
        let row: (i64,) = sqlx::query_as(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?"
        ).bind(t).fetch_one(&pool).await.unwrap();
        assert_eq!(row.0, 1, "missing table: {t}");
    }
}
```

- [ ] **Step 2: Set up `src/lib.rs` so integration tests can import**

Create `src/lib.rs`:

```rust
pub mod cli;
pub mod db;
pub mod error;
pub mod ids;
pub mod telemetry;
```

Modify `src/main.rs` top:

```rust
use clap::Parser;
use witmcc::{cli, db, error, telemetry};

fn main() -> error::Result<()> {
    let cli = cli::Cli::parse();
    telemetry::init(&cli.log_format, cli.verbose);
    let rt = tokio::runtime::Runtime::new().map_err(anyhow::Error::from)?;
    rt.block_on(async move {
        match cli.command {
            cli::Command::InitDb => init_db(&cli.db_path).await,
            cli::Command::Ingest { .. } => Ok(()), // wired in later tasks
            cli::Command::Serve   { .. } => Ok(()), // wired in later tasks
        }
    })
}

async fn init_db(path: &std::path::Path) -> error::Result<()> {
    let url = format!("sqlite://{}?mode=rwc", path.display());
    let pool = db::connect(&url).await?;
    db::migrate(&pool).await?;
    tracing::info!(?path, "init-db complete");
    Ok(())
}
```

Update `Cargo.toml`:

```toml
[[bin]]
name = "witmcc"
path = "src/main.rs"

[lib]
name = "witmcc"
path = "src/lib.rs"
```

- [ ] **Step 3: Write `src/db/mod.rs`**

```rust
use sqlx::{sqlite::SqlitePoolOptions, ConnectOptions, SqlitePool};
use std::str::FromStr;

use crate::error::Result;

pub async fn connect(url: &str) -> Result<SqlitePool> {
    let opts = sqlx::sqlite::SqliteConnectOptions::from_str(url)?
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(std::time::Duration::from_millis(5000))
        .create_if_missing(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(opts).await?;
    Ok(pool)
}

pub async fn migrate(pool: &SqlitePool) -> Result<()> {
    sqlx::migrate!("./migrations").run(pool).await?;
    Ok(())
}
```

- [ ] **Step 4: Run tests**

Run: `SQLX_OFFLINE=false cargo test --test db_init`
Expected: PASS.

- [ ] **Step 5: Smoke `init-db`**

Run:
```
cargo run -- --db-path /tmp/witmcc-smoke.sqlite init-db && \
  sqlite3 /tmp/witmcc-smoke.sqlite '.tables' && rm /tmp/witmcc-smoke.sqlite*
```
Expected: lists `graph_edge graph_node ingest_run observed_event raw_event _sqlx_migrations`.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml src/lib.rs src/main.rs src/db/mod.rs tests/db_init.rs
git commit -m "feat(db): connect (WAL/FK/busy_timeout) + sqlx migrate; wire init-db"
```

---

## Task 7: Model types (pure)

**Files:**
- Create: `src/model/mod.rs`, `raw.rs`, `observed.rs`, `graph.rs`, `meta.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: `src/model/mod.rs`**

```rust
pub mod raw;
pub mod observed;
pub mod graph;
pub mod meta;
```

- [ ] **Step 2: `src/model/meta.rs`**

```rust
use serde::Serialize;

pub const SCHEMA_VERSION: &str = "0.1.0";
pub const PARSER_VERSION_TRANSCRIPT: &str = "transcript@0.1.0";
pub const COLLECTION_PROFILE: &str = "local_transcript_slice1";

#[derive(Debug, Serialize)]
pub struct ResponseMeta {
    pub schema_version: &'static str,
    pub collection_profile: &'static str,
    pub redaction_policy: Option<&'static str>, // always None in slice-1
    pub generated_at: chrono::DateTime<chrono::Utc>,
    pub next_cursor: Option<String>,            // always None in slice-1
}

impl ResponseMeta {
    pub fn now() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            collection_profile: COLLECTION_PROFILE,
            redaction_policy: None,
            generated_at: chrono::Utc::now(),
            next_cursor: None,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Envelope<T: Serialize> {
    pub meta: ResponseMeta,
    pub data: T,
}
```

- [ ] **Step 3: `src/model/raw.rs`**

```rust
use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct RawEvent {
    pub raw_event_id: String,
    pub ingest_run_id: String,
    pub source_type: String,         // "claude_transcript" | "unparseable"
    pub source_uri: String,
    pub source_line_no: i64,
    pub source_byte_offset: i64,
    pub payload_sha256: String,
    pub payload: Vec<u8>,
    pub parse_error: Option<String>,
    pub captured_at: DateTime<Utc>,
}
```

- [ ] **Step 4: `src/model/observed.rs`**

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Actor { User, Assistant, System, Hook, Tool }

impl Actor { pub fn as_str(&self) -> &'static str { match self {
    Actor::User => "user", Actor::Assistant => "assistant", Actor::System => "system",
    Actor::Hook => "hook", Actor::Tool => "tool",
} } }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    UserMessage, AssistantMessage, Thinking, ToolCall, ToolResult,
    HookEvent, SystemSummary, SessionState, FileHistorySnapshot,
    AttachmentMeta, Unknown,
}

impl EventKind { pub fn as_str(&self) -> &'static str { match self {
    EventKind::UserMessage => "user_message", EventKind::AssistantMessage => "assistant_message",
    EventKind::Thinking => "thinking", EventKind::ToolCall => "tool_call",
    EventKind::ToolResult => "tool_result", EventKind::HookEvent => "hook_event",
    EventKind::SystemSummary => "system_summary", EventKind::SessionState => "session_state",
    EventKind::FileHistorySnapshot => "file_history_snapshot",
    EventKind::AttachmentMeta => "attachment_meta", EventKind::Unknown => "unknown",
} } }

#[derive(Debug, Clone, Default)]
pub struct ObservedEvent {
    pub event_id: String,
    pub raw_event_id: String,
    pub schema_version: String,
    pub session_id: String,
    pub event_uuid: Option<String>,
    pub parent_uuid: Option<String>,
    pub observed_at: DateTime<Utc>,
    pub actor: Actor,
    pub kind: EventKind,
    pub subkind: Option<String>,
    pub tool_use_id: Option<String>,
    pub tool_name: Option<String>,
    pub request_id: Option<String>,
    pub message_id: Option<String>,
    pub turn_id: Option<String>,
    pub source_tool_assistant_uuid: Option<String>,
    pub source_tool_use_id: Option<String>,
    pub is_sidechain: bool,
    pub is_meta: bool,
    pub cwd: Option<String>,
    pub git_branch: Option<String>,
    pub user_type: Option<String>,
    pub entrypoint: Option<String>,
    pub cc_version: Option<String>,
    pub payload: serde_json::Value,
    pub parser_version: String,
}

impl Default for Actor { fn default() -> Self { Actor::System } }
impl Default for EventKind { fn default() -> Self { EventKind::Unknown } }
```

- [ ] **Step 5: `src/model/graph.rs`**

```rust
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
pub struct GraphNode {
    pub node_id: String,
    pub schema_version: String,
    pub session_id: String,
    pub node_kind: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub merge_keys: Value,
    pub source_event_ids: Vec<String>,
    pub source_uris: Vec<String>,
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct GraphEdge {
    pub edge_id: String,
    pub schema_version: String,
    pub session_id: String,
    pub from_node_id: String,
    pub to_node_id: String,
    pub edge_kind: String,
    pub origin: String,             // "deterministic"
    pub attributes: Value,
}
```

- [ ] **Step 6: Wire `pub mod model;` in `src/lib.rs`, ensure `cargo build` succeeds**

```bash
cargo build
```

- [ ] **Step 7: Commit**

```bash
git add src/model src/lib.rs
git commit -m "feat(model): pure types for raw/observed/graph + response envelope"
```

---

## Task 8: JSONL streaming parser

**Files:**
- Create: `src/ingest/mod.rs`
- Create: `src/ingest/transcript.rs`
- Create: `tests/fixtures/transcripts/minimal_session.jsonl`
- Test: `tests/parser.rs`

- [ ] **Step 1: Create the minimal fixture**

`tests/fixtures/transcripts/minimal_session.jsonl` (exact content, 5 lines):

```jsonl
{"type":"user","uuid":"u1","parentUuid":null,"sessionId":"sess-A","timestamp":"2026-05-19T03:00:00Z","cwd":"/tmp","gitBranch":"main","entrypoint":"cli","userType":"external","version":"2.1.144","isSidechain":false,"promptId":"p1","message":{"role":"user","content":"hello"}}
{"type":"assistant","uuid":"a1","parentUuid":"u1","sessionId":"sess-A","timestamp":"2026-05-19T03:00:01Z","cwd":"/tmp","gitBranch":"main","entrypoint":"cli","userType":"external","version":"2.1.144","isSidechain":false,"requestId":"req_1","message":{"id":"msg_1","model":"claude-opus-4-7","type":"message","role":"assistant","stop_reason":"tool_use","content":[{"type":"text","text":"calling tool"},{"type":"tool_use","id":"toolu_x","name":"Bash","input":{"command":"ls"}}]}}
{"type":"user","uuid":"u2","parentUuid":"a1","sessionId":"sess-A","timestamp":"2026-05-19T03:00:02Z","cwd":"/tmp","gitBranch":"main","entrypoint":"cli","userType":"external","version":"2.1.144","isSidechain":false,"promptId":"p1","sourceToolAssistantUUID":"a1","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_x","content":"ok","is_error":false}]}}
{"type":"assistant","uuid":"a2","parentUuid":"u2","sessionId":"sess-A","timestamp":"2026-05-19T03:00:03Z","cwd":"/tmp","gitBranch":"main","entrypoint":"cli","userType":"external","version":"2.1.144","isSidechain":false,"requestId":"req_2","message":{"id":"msg_2","model":"claude-opus-4-7","type":"message","role":"assistant","stop_reason":"end_turn","content":[{"type":"text","text":"done"}]}}
{"type":"permission-mode","permissionMode":"default","sessionId":"sess-A"}
```

- [ ] **Step 2: Write failing test `tests/parser.rs`**

```rust
use witmcc::ingest::transcript::{stream_file, ParsedRecord};
use futures::StreamExt;
use std::path::Path;

#[tokio::test]
async fn parses_five_record_types_in_minimal_fixture() {
    let p = Path::new("tests/fixtures/transcripts/minimal_session.jsonl");
    let mut stream = Box::pin(stream_file(p).await.unwrap());
    let mut kinds = vec![];
    while let Some(item) = stream.next().await {
        let (_meta, rec) = item.unwrap();
        kinds.push(match rec {
            ParsedRecord::User(_)        => "user",
            ParsedRecord::Assistant(_)   => "assistant",
            ParsedRecord::Attachment(_)  => "attachment",
            ParsedRecord::SystemMsg(_)   => "system",
            ParsedRecord::PermissionMode(_) => "permission-mode",
            ParsedRecord::LastPrompt(_)  => "last-prompt",
            ParsedRecord::FileHistorySnapshot(_) => "file-history-snapshot",
            ParsedRecord::Unknown(_)     => "unknown",
        });
    }
    assert_eq!(kinds, vec!["user","assistant","user","assistant","permission-mode"]);
}
```

- [ ] **Step 3: Write `src/ingest/mod.rs` and `src/ingest/transcript.rs`**

`src/ingest/mod.rs`:

```rust
pub mod transcript;
```

`src/ingest/transcript.rs`:

```rust
use chrono::{DateTime, Utc};
use futures::stream::{Stream, StreamExt};
use serde::Deserialize;
use serde_json::Value;
use std::path::{Path, PathBuf};
use tokio::io::AsyncBufReadExt;

use crate::error::WitmccError;

pub const MAX_LINE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct LineMeta {
    pub source_uri: PathBuf,
    pub line_no: u64,
    pub byte_offset: u64,
    pub raw: Vec<u8>,                       // original line bytes (without newline)
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum ParsedRecord {
    #[serde(rename = "user")]                    User(UserRecord),
    #[serde(rename = "assistant")]               Assistant(AssistantRecord),
    #[serde(rename = "attachment")]              Attachment(AttachmentRecord),
    #[serde(rename = "system")]                  SystemMsg(SystemRecord),
    #[serde(rename = "permission-mode")]         PermissionMode(PermissionModeRecord),
    #[serde(rename = "last-prompt")]             LastPrompt(LastPromptRecord),
    #[serde(rename = "file-history-snapshot")]   FileHistorySnapshot(FileHistorySnapshotRecord),
    #[serde(other)]                              Unknown,
}

// Note: `Unknown` cannot carry the inner Value with `#[serde(other)]`; for
// slice-1 we capture the inner separately in the wrapping `parse_line`.
#[derive(Debug, Deserialize)]
pub struct UserRecord {
    pub uuid: String,
    #[serde(rename = "parentUuid")]   pub parent_uuid: Option<String>,
    #[serde(rename = "sessionId")]    pub session_id: String,
    pub timestamp: DateTime<Utc>,
    pub cwd: Option<String>,
    #[serde(rename = "gitBranch")]    pub git_branch: Option<String>,
    pub entrypoint: Option<String>,
    #[serde(rename = "userType")]     pub user_type: Option<String>,
    pub version: Option<String>,
    #[serde(rename = "isSidechain")]  #[serde(default)] pub is_sidechain: bool,
    #[serde(rename = "isMeta")]       #[serde(default)] pub is_meta: bool,
    #[serde(rename = "promptId")]     pub prompt_id: Option<String>,
    #[serde(rename = "sourceToolAssistantUUID")] pub source_tool_assistant_uuid: Option<String>,
    #[serde(rename = "sourceToolUseID")] pub source_tool_use_id: Option<String>,
    pub message: Value,
}

#[derive(Debug, Deserialize)]
pub struct AssistantRecord {
    pub uuid: String,
    #[serde(rename = "parentUuid")]   pub parent_uuid: Option<String>,
    #[serde(rename = "sessionId")]    pub session_id: String,
    pub timestamp: DateTime<Utc>,
    pub cwd: Option<String>,
    #[serde(rename = "gitBranch")]    pub git_branch: Option<String>,
    pub entrypoint: Option<String>,
    #[serde(rename = "userType")]     pub user_type: Option<String>,
    pub version: Option<String>,
    #[serde(rename = "isSidechain")]  #[serde(default)] pub is_sidechain: bool,
    #[serde(rename = "requestId")]    pub request_id: Option<String>,
    pub message: Value,
}

#[derive(Debug, Deserialize)]
pub struct AttachmentRecord {
    pub uuid: String,
    #[serde(rename = "parentUuid")]   pub parent_uuid: Option<String>,
    #[serde(rename = "sessionId")]    pub session_id: String,
    pub timestamp: DateTime<Utc>,
    pub cwd: Option<String>,
    #[serde(rename = "gitBranch")]    pub git_branch: Option<String>,
    pub entrypoint: Option<String>,
    #[serde(rename = "userType")]     pub user_type: Option<String>,
    pub version: Option<String>,
    #[serde(rename = "isSidechain")]  #[serde(default)] pub is_sidechain: bool,
    pub attachment: Value,
}

#[derive(Debug, Deserialize)]
pub struct SystemRecord {
    pub uuid: String,
    #[serde(rename = "parentUuid")]   pub parent_uuid: Option<String>,
    #[serde(rename = "sessionId")]    pub session_id: String,
    pub timestamp: DateTime<Utc>,
    pub subtype: Option<String>,
    #[serde(rename = "toolUseID")]    pub tool_use_id: Option<String>,
    #[serde(flatten)] pub rest: Value,
}

#[derive(Debug, Deserialize)]
pub struct PermissionModeRecord {
    #[serde(rename = "sessionId")] pub session_id: String,
    #[serde(rename = "permissionMode")] pub permission_mode: String,
}

#[derive(Debug, Deserialize)]
pub struct LastPromptRecord {
    #[serde(rename = "sessionId")] pub session_id: String,
    #[serde(rename = "leafUuid")] pub leaf_uuid: String,
}

#[derive(Debug, Deserialize)]
pub struct FileHistorySnapshotRecord {
    #[serde(rename = "messageId")] pub message_id: String,
    #[serde(rename = "isSnapshotUpdate")] pub is_snapshot_update: bool,
    pub snapshot: Value,
}

pub async fn stream_file(path: &Path)
    -> std::io::Result<impl Stream<Item = Result<(LineMeta, ParsedRecord), WitmccError>>>
{
    let file = tokio::fs::File::open(path).await?;
    let reader = tokio::io::BufReader::with_capacity(64 * 1024, file);
    let source_uri = path.to_path_buf();
    Ok(LineStream::new(reader, source_uri))
}

struct LineStream<R> {
    inner: tokio::io::Lines<R>,
    line_no: u64,
    byte_offset: u64,
    source_uri: PathBuf,
}

impl<R: AsyncBufReadExt + Unpin> LineStream<R> {
    fn new(reader: R, source_uri: PathBuf) -> impl Stream<Item = Result<(LineMeta, ParsedRecord), WitmccError>> {
        let lines = reader.lines();
        // Use stream::unfold to track offsets between awaits.
        futures::stream::unfold(
            State { lines, line_no: 0, byte_offset: 0, source_uri },
            |mut st| async move {
                match st.lines.next_line().await {
                    Ok(Some(line)) => {
                        st.line_no += 1;
                        let bytes = line.as_bytes();
                        let mut bytes_vec = bytes.to_vec();
                        if bytes_vec.len() > MAX_LINE_BYTES { bytes_vec.truncate(MAX_LINE_BYTES); }
                        let meta = LineMeta {
                            source_uri: st.source_uri.clone(),
                            line_no: st.line_no,
                            byte_offset: st.byte_offset,
                            raw: bytes_vec,
                        };
                        let parsed: Result<ParsedRecord, _> = serde_json::from_str(&line);
                        st.byte_offset += bytes.len() as u64 + 1; // +1 for newline
                        match parsed {
                            Ok(rec) => Some((Ok((meta, rec)), st)),
                            Err(e) => Some((Err(WitmccError::ParseLine {
                                source_uri: st.source_uri.display().to_string(),
                                line_no: st.line_no,
                                message: e.to_string(),
                            }), st)),
                        }
                    }
                    Ok(None) => None,
                    Err(e) => Some((Err(WitmccError::Io {
                        path: st.source_uri.clone(), source: e,
                    }), st)),
                }
            },
        )
    }
}

struct State<R> {
    lines: tokio::io::Lines<R>,
    line_no: u64,
    byte_offset: u64,
    source_uri: PathBuf,
}
```

(If the `LineStream::new` API fights you, simplify by writing the unfold inline at `stream_file`. The goal is a `Stream<Item = Result<(LineMeta, ParsedRecord), WitmccError>>`. Don't get fancy.)

- [ ] **Step 4: Wire `pub mod ingest;` in `src/lib.rs`**

- [ ] **Step 5: Run test**

Run: `cargo test --test parser`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/ingest src/lib.rs tests/fixtures tests/parser.rs
git commit -m "feat(ingest): JSONL streaming parser + ParsedRecord taxonomy"
```

---

## Task 9: `ParsedRecord` → `ObservedEvent` mapping

**Files:**
- Create: `src/ingest/mapping.rs`
- Modify: `src/ingest/mod.rs`
- Test: `tests/mapping.rs`

- [ ] **Step 1: Write failing test `tests/mapping.rs`**

```rust
use witmcc::ingest::transcript::{stream_file};
use witmcc::ingest::mapping::map_record;
use witmcc::model::observed::{Actor, EventKind};
use futures::StreamExt;

#[tokio::test]
async fn maps_minimal_fixture_to_seven_observed_events() {
    let p = std::path::Path::new("tests/fixtures/transcripts/minimal_session.jsonl");
    let mut stream = Box::pin(stream_file(p).await.unwrap());
    let mut events = vec![];
    let mut gen = witmcc::ids::MonotonicUlidGen::new();
    while let Some(item) = stream.next().await {
        let (meta, rec) = item.unwrap();
        let raw_id = gen.next();
        events.extend(map_record(&meta, &rec, &raw_id, &mut gen).unwrap());
    }
    // Expected breakdown for the 5-line fixture:
    //   user(string)          → 1 user_message
    //   assistant(text+tool)  → 1 assistant_message + 1 tool_call
    //   user(tool_result)     → 1 tool_result
    //   assistant(text)       → 1 assistant_message
    //   permission-mode       → 1 session_state
    //                  total  = 6
    assert_eq!(events.len(), 6, "{events:#?}");
    let kinds: Vec<_> = events.iter().map(|e| e.kind).collect();
    assert_eq!(kinds, vec![
        EventKind::UserMessage,
        EventKind::AssistantMessage, EventKind::ToolCall,
        EventKind::ToolResult,
        EventKind::AssistantMessage,
        EventKind::SessionState,
    ]);
    // Spot-check correlation keys.
    let tc = events.iter().find(|e| e.kind == EventKind::ToolCall).unwrap();
    assert_eq!(tc.tool_use_id.as_deref(), Some("toolu_x"));
    assert_eq!(tc.tool_name.as_deref(), Some("Bash"));
    assert_eq!(tc.actor, Actor::Assistant);
    let tr = events.iter().find(|e| e.kind == EventKind::ToolResult).unwrap();
    assert_eq!(tr.tool_use_id.as_deref(), Some("toolu_x"));
    assert_eq!(tr.source_tool_assistant_uuid.as_deref(), Some("a1"));
}
```

- [ ] **Step 2: Write `src/ingest/mapping.rs`**

```rust
use serde_json::{json, Value};

use crate::error::{Result, WitmccError};
use crate::ids::MonotonicUlidGen;
use crate::ingest::transcript::{LineMeta, ParsedRecord, AssistantRecord, UserRecord};
use crate::model::meta::{PARSER_VERSION_TRANSCRIPT, SCHEMA_VERSION};
use crate::model::observed::{Actor, EventKind, ObservedEvent};

pub fn map_record(
    meta: &LineMeta,
    rec: &ParsedRecord,
    raw_event_id: &str,
    gen: &mut MonotonicUlidGen,
) -> Result<Vec<ObservedEvent>> {
    match rec {
        ParsedRecord::User(u)        => Ok(map_user(meta, u, raw_event_id, gen)),
        ParsedRecord::Assistant(a)   => Ok(map_assistant(meta, a, raw_event_id, gen)),
        ParsedRecord::Attachment(_)  => Ok(vec![attachment_meta(meta, raw_event_id, gen, rec)]),
        ParsedRecord::SystemMsg(_)   => Ok(vec![system_summary(meta, raw_event_id, gen, rec)]),
        ParsedRecord::PermissionMode(p) => Ok(vec![session_state(meta, raw_event_id, gen,
                                            &p.session_id, "permission_mode",
                                            json!({"permissionMode": p.permission_mode}))]),
        ParsedRecord::LastPrompt(l)  => Ok(vec![session_state(meta, raw_event_id, gen,
                                            &l.session_id, "last_prompt",
                                            json!({"leafUuid": l.leaf_uuid}))]),
        ParsedRecord::FileHistorySnapshot(f) => Ok(vec![file_history(meta, raw_event_id, gen, f)]),
        ParsedRecord::Unknown        => Err(WitmccError::Invalid("unknown record type".into())),
    }
}

fn base(meta: &LineMeta, raw_event_id: &str, gen: &mut MonotonicUlidGen) -> ObservedEvent {
    ObservedEvent {
        event_id: gen.next(),
        raw_event_id: raw_event_id.into(),
        schema_version: SCHEMA_VERSION.into(),
        parser_version: PARSER_VERSION_TRANSCRIPT.into(),
        observed_at: chrono::Utc::now(),  // overwritten by caller
        payload: Value::Null,
        ..Default::default()
    }
}

fn map_user(meta: &LineMeta, u: &UserRecord, raw_event_id: &str, gen: &mut MonotonicUlidGen)
    -> Vec<ObservedEvent>
{
    // tool_result branch: content is array containing {type:"tool_result", tool_use_id:..}
    if let Some(arr) = u.message.get("content").and_then(|c| c.as_array()) {
        let mut out = Vec::new();
        for (ord, item) in arr.iter().enumerate() {
            if item.get("type").and_then(|t| t.as_str()) == Some("tool_result") {
                let mut e = base(meta, raw_event_id, gen);
                e.session_id = u.session_id.clone();
                e.event_uuid = Some(u.uuid.clone());
                e.parent_uuid = u.parent_uuid.clone();
                e.observed_at = u.timestamp;
                e.actor = Actor::System;
                e.kind = EventKind::ToolResult;
                e.tool_use_id = item.get("tool_use_id").and_then(|x| x.as_str()).map(String::from);
                e.turn_id = u.prompt_id.clone();
                e.source_tool_assistant_uuid = u.source_tool_assistant_uuid.clone();
                e.source_tool_use_id = u.source_tool_use_id.clone();
                e.is_sidechain = u.is_sidechain;
                e.is_meta = u.is_meta;
                e.cwd = u.cwd.clone();
                e.git_branch = u.git_branch.clone();
                e.user_type = u.user_type.clone();
                e.entrypoint = u.entrypoint.clone();
                e.cc_version = u.version.clone();
                e.payload = json!({"content_ordinal": ord, "tool_result": item});
                out.push(e);
            } else if item.get("type").and_then(|t| t.as_str()) == Some("text") {
                out.push(user_message(meta, u, raw_event_id, gen, json!({"content_ordinal": ord, "text": item.get("text")})));
            }
        }
        if !out.is_empty() { return out; }
    }
    // string content branch
    vec![user_message(meta, u, raw_event_id, gen, json!({"content": u.message.get("content")}))]
}

fn user_message(meta: &LineMeta, u: &UserRecord, raw_event_id: &str, gen: &mut MonotonicUlidGen, payload: Value)
    -> ObservedEvent
{
    let mut e = base(meta, raw_event_id, gen);
    e.session_id = u.session_id.clone();
    e.event_uuid = Some(u.uuid.clone());
    e.parent_uuid = u.parent_uuid.clone();
    e.observed_at = u.timestamp;
    e.actor = Actor::User;
    e.kind = EventKind::UserMessage;
    e.turn_id = u.prompt_id.clone();
    e.is_sidechain = u.is_sidechain;
    e.is_meta = u.is_meta;
    e.cwd = u.cwd.clone();
    e.git_branch = u.git_branch.clone();
    e.user_type = u.user_type.clone();
    e.entrypoint = u.entrypoint.clone();
    e.cc_version = u.version.clone();
    e.payload = payload;
    e
}

fn map_assistant(meta: &LineMeta, a: &AssistantRecord, raw_event_id: &str, gen: &mut MonotonicUlidGen)
    -> Vec<ObservedEvent>
{
    let mut out = Vec::new();
    let arr = a.message.get("content").and_then(|c| c.as_array()).cloned().unwrap_or_default();
    let message_id = a.message.get("id").and_then(|x| x.as_str()).map(String::from);
    for (ord, item) in arr.iter().enumerate() {
        let ty = item.get("type").and_then(|t| t.as_str()).unwrap_or("");
        let mut e = base(meta, raw_event_id, gen);
        e.session_id = a.session_id.clone();
        e.event_uuid = Some(a.uuid.clone());
        e.parent_uuid = a.parent_uuid.clone();
        e.observed_at = a.timestamp;
        e.actor = Actor::Assistant;
        e.request_id = a.request_id.clone();
        e.message_id = message_id.clone();
        e.is_sidechain = a.is_sidechain;
        e.cwd = a.cwd.clone();
        e.git_branch = a.git_branch.clone();
        e.user_type = a.user_type.clone();
        e.entrypoint = a.entrypoint.clone();
        e.cc_version = a.version.clone();
        match ty {
            "text" => {
                e.kind = EventKind::AssistantMessage;
                e.payload = json!({"content_ordinal": ord, "text": item.get("text")});
            }
            "thinking" => {
                e.kind = EventKind::Thinking;
                e.payload = json!({"content_ordinal": ord, "thinking": item.get("thinking"), "signature": item.get("signature")});
            }
            "tool_use" => {
                e.kind = EventKind::ToolCall;
                e.tool_use_id = item.get("id").and_then(|x| x.as_str()).map(String::from);
                e.tool_name = item.get("name").and_then(|x| x.as_str()).map(String::from);
                e.payload = json!({"content_ordinal": ord, "input": item.get("input")});
            }
            _ => {
                e.kind = EventKind::Unknown;
                e.payload = json!({"content_ordinal": ord, "raw": item});
            }
        }
        out.push(e);
    }
    out
}

fn attachment_meta(meta: &LineMeta, raw_event_id: &str, gen: &mut MonotonicUlidGen, rec: &ParsedRecord) -> ObservedEvent {
    let ParsedRecord::Attachment(a) = rec else { unreachable!() };
    let subtype = a.attachment.get("type").and_then(|t| t.as_str()).unwrap_or("");
    let is_hook = subtype == "hook_success" || subtype == "hook_additional_context";
    let mut e = base(meta, raw_event_id, gen);
    e.session_id = a.session_id.clone();
    e.event_uuid = Some(a.uuid.clone());
    e.parent_uuid = a.parent_uuid.clone();
    e.observed_at = a.timestamp;
    e.is_sidechain = a.is_sidechain;
    e.cwd = a.cwd.clone(); e.git_branch = a.git_branch.clone();
    e.user_type = a.user_type.clone(); e.entrypoint = a.entrypoint.clone(); e.cc_version = a.version.clone();
    e.subkind = Some(subtype.into());
    if is_hook {
        e.actor = Actor::Hook;
        e.kind = EventKind::HookEvent;
        e.tool_use_id = a.attachment.get("toolUseID").and_then(|x| x.as_str()).map(String::from);
        e.tool_name = a.attachment.get("hookName").and_then(|x| x.as_str()).map(String::from);
    } else {
        e.actor = Actor::System;
        e.kind = EventKind::AttachmentMeta;
    }
    e.payload = a.attachment.clone();
    e
}

fn system_summary(meta: &LineMeta, raw_event_id: &str, gen: &mut MonotonicUlidGen, rec: &ParsedRecord) -> ObservedEvent {
    let ParsedRecord::SystemMsg(s) = rec else { unreachable!() };
    let mut e = base(meta, raw_event_id, gen);
    e.session_id = s.session_id.clone();
    e.event_uuid = Some(s.uuid.clone());
    e.parent_uuid = s.parent_uuid.clone();
    e.observed_at = s.timestamp;
    e.actor = Actor::System;
    e.kind = EventKind::SystemSummary;
    e.subkind = s.subtype.clone();
    e.tool_use_id = s.tool_use_id.clone();
    e.payload = s.rest.clone();
    e
}

fn session_state(meta: &LineMeta, raw_event_id: &str, gen: &mut MonotonicUlidGen,
                 session_id: &str, subkind: &str, payload: Value) -> ObservedEvent
{
    let mut e = base(meta, raw_event_id, gen);
    e.session_id = session_id.into();
    e.observed_at = chrono::Utc::now();
    e.actor = Actor::System;
    e.kind = EventKind::SessionState;
    e.subkind = Some(subkind.into());
    e.payload = payload;
    e
}

fn file_history(meta: &LineMeta, raw_event_id: &str, gen: &mut MonotonicUlidGen,
                f: &crate::ingest::transcript::FileHistorySnapshotRecord) -> ObservedEvent
{
    let mut e = base(meta, raw_event_id, gen);
    e.observed_at = chrono::Utc::now();
    e.actor = Actor::System;
    e.kind = EventKind::FileHistorySnapshot;
    e.message_id = Some(f.message_id.clone());
    e.payload = json!({"isSnapshotUpdate": f.is_snapshot_update, "snapshot": f.snapshot});
    e
}
```

Add `pub mod mapping;` in `src/ingest/mod.rs`.

- [ ] **Step 3: Run test**

Run: `cargo test --test mapping`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add src/ingest/mapping.rs src/ingest/mod.rs tests/mapping.rs
git commit -m "feat(ingest): ParsedRecord -> ObservedEvent mapping (content split, hook node)"
```

---

## Task 10: Repos — `raw_event` insert with dedupe

**Files:**
- Create: `src/db/repo_raw.rs`, `src/db/repo_runs.rs`
- Modify: `src/db/mod.rs`
- Test: `tests/repo_raw.rs`

- [ ] **Step 1: Write failing test `tests/repo_raw.rs`**

```rust
use sqlx::sqlite::SqlitePoolOptions;
use witmcc::db::{migrate, repo_raw, repo_runs};
use chrono::Utc;

#[tokio::test]
async fn idempotent_insert() {
    let pool = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
    migrate(&pool).await.unwrap();
    let run_id = repo_runs::start(&pool).await.unwrap();
    let row = repo_raw::NewRaw {
        raw_event_id: "01HXAAA".into(),
        ingest_run_id: run_id.clone(),
        source_type: "claude_transcript".into(),
        source_uri: "/tmp/x.jsonl".into(),
        source_line_no: 1,
        source_byte_offset: 0,
        payload_sha256: "deadbeef".into(),
        payload: b"hello".to_vec(),
        parse_error: None,
        captured_at: Utc::now(),
    };
    let inserted_first = repo_raw::insert_dedup(&pool, &row).await.unwrap();
    let inserted_second = repo_raw::insert_dedup(&pool, &row).await.unwrap();
    assert!(inserted_first, "first insert should report newly inserted");
    assert!(!inserted_second, "second insert with identical (uri,line,sha) is a no-op");
    let count: (i64,) = sqlx::query_as("SELECT count(*) FROM raw_event")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(count.0, 1);
}
```

- [ ] **Step 2: Write `src/db/repo_runs.rs`**

```rust
use crate::error::Result;
use crate::ids::MonotonicUlidGen;
use sqlx::SqlitePool;

pub async fn start(pool: &SqlitePool) -> Result<String> {
    let run_id = MonotonicUlidGen::new().next();
    sqlx::query("INSERT INTO ingest_run(run_id, started_at, status) VALUES(?, ?, 'running')")
        .bind(&run_id).bind(chrono::Utc::now().to_rfc3339())
        .execute(pool).await?;
    Ok(run_id)
}

pub async fn finish(pool: &SqlitePool, run_id: &str, status: &str, stats: serde_json::Value) -> Result<()> {
    sqlx::query("UPDATE ingest_run SET finished_at=?, status=?, stats=? WHERE run_id=?")
        .bind(chrono::Utc::now().to_rfc3339()).bind(status).bind(stats.to_string()).bind(run_id)
        .execute(pool).await?;
    Ok(())
}
```

- [ ] **Step 3: Write `src/db/repo_raw.rs`**

```rust
use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use crate::error::Result;

pub struct NewRaw {
    pub raw_event_id: String,
    pub ingest_run_id: String,
    pub source_type: String,
    pub source_uri: String,
    pub source_line_no: i64,
    pub source_byte_offset: i64,
    pub payload_sha256: String,
    pub payload: Vec<u8>,
    pub parse_error: Option<String>,
    pub captured_at: DateTime<Utc>,
}

/// Returns true if a new row was inserted; false if the
/// `(source_uri, source_line_no, payload_sha256)` triple already existed.
pub async fn insert_dedup(pool: &SqlitePool, r: &NewRaw) -> Result<bool> {
    let res = sqlx::query(
        "INSERT INTO raw_event(
            raw_event_id, ingest_run_id, source_type, source_uri,
            source_line_no, source_byte_offset, payload_sha256, payload,
            parse_error, captured_at)
         VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(source_uri, source_line_no, payload_sha256) DO NOTHING")
        .bind(&r.raw_event_id).bind(&r.ingest_run_id).bind(&r.source_type).bind(&r.source_uri)
        .bind(r.source_line_no).bind(r.source_byte_offset).bind(&r.payload_sha256).bind(&r.payload)
        .bind(&r.parse_error).bind(r.captured_at.to_rfc3339())
        .execute(pool).await?;
    Ok(res.rows_affected() > 0)
}
```

- [ ] **Step 4: Wire `pub mod repo_raw; pub mod repo_runs;` in `src/db/mod.rs`**

- [ ] **Step 5: Run test**

Run: `cargo test --test repo_raw`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/db/repo_raw.rs src/db/repo_runs.rs src/db/mod.rs tests/repo_raw.rs
git commit -m "feat(db): repo_raw insert_dedup + repo_runs lifecycle"
```

---

## Task 11: Repos — `observed_event` insert + read helpers

**Files:**
- Create: `src/db/repo_observed.rs`
- Modify: `src/db/mod.rs`
- Test: `tests/repo_observed.rs`

- [ ] **Step 1: Write failing test `tests/repo_observed.rs`**

```rust
use witmcc::db::{migrate, repo_observed, repo_runs, repo_raw};
use witmcc::model::observed::{Actor, EventKind, ObservedEvent};
use witmcc::model::meta::{SCHEMA_VERSION, PARSER_VERSION_TRANSCRIPT};
use sqlx::sqlite::SqlitePoolOptions;

#[tokio::test]
async fn insert_and_list_session_events() {
    let pool = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
    migrate(&pool).await.unwrap();
    let run_id = repo_runs::start(&pool).await.unwrap();
    let raw = repo_raw::NewRaw {
        raw_event_id: "raw1".into(),
        ingest_run_id: run_id,
        source_type: "claude_transcript".into(),
        source_uri: "/tmp/x.jsonl".into(),
        source_line_no: 1, source_byte_offset: 0,
        payload_sha256: "abc".into(), payload: b"{}".to_vec(),
        parse_error: None, captured_at: chrono::Utc::now(),
    };
    repo_raw::insert_dedup(&pool, &raw).await.unwrap();

    let e = ObservedEvent {
        event_id: "ev1".into(), raw_event_id: "raw1".into(),
        schema_version: SCHEMA_VERSION.into(), parser_version: PARSER_VERSION_TRANSCRIPT.into(),
        session_id: "sess".into(),
        observed_at: chrono::Utc::now(),
        actor: Actor::User, kind: EventKind::UserMessage,
        payload: serde_json::json!({"x":1}),
        ..Default::default()
    };
    repo_observed::insert(&pool, &e).await.unwrap();

    let evs = repo_observed::list_session(&pool, "sess", 100).await.unwrap();
    assert_eq!(evs.len(), 1);
    assert_eq!(evs[0].event_id, "ev1");
}
```

- [ ] **Step 2: Write `src/db/repo_observed.rs`**

```rust
use sqlx::{SqlitePool, Row};
use crate::error::Result;
use crate::model::observed::{Actor, EventKind, ObservedEvent};

pub async fn insert(pool: &SqlitePool, e: &ObservedEvent) -> Result<()> {
    sqlx::query(
        "INSERT INTO observed_event(
            event_id, raw_event_id, schema_version, session_id, event_uuid, parent_uuid,
            observed_at, actor, kind, subkind, tool_use_id, tool_name, request_id,
            message_id, turn_id, source_tool_assistant_uuid, source_tool_use_id,
            is_sidechain, is_meta, cwd, git_branch, user_type, entrypoint, cc_version,
            payload, parser_version)
         VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
        .bind(&e.event_id).bind(&e.raw_event_id).bind(&e.schema_version).bind(&e.session_id)
        .bind(&e.event_uuid).bind(&e.parent_uuid).bind(e.observed_at.to_rfc3339())
        .bind(e.actor.as_str()).bind(e.kind.as_str()).bind(&e.subkind)
        .bind(&e.tool_use_id).bind(&e.tool_name).bind(&e.request_id).bind(&e.message_id)
        .bind(&e.turn_id).bind(&e.source_tool_assistant_uuid).bind(&e.source_tool_use_id)
        .bind(e.is_sidechain as i64).bind(e.is_meta as i64)
        .bind(&e.cwd).bind(&e.git_branch).bind(&e.user_type).bind(&e.entrypoint).bind(&e.cc_version)
        .bind(e.payload.to_string()).bind(&e.parser_version)
        .execute(pool).await?;
    Ok(())
}

pub struct SessionRow {
    pub session_id: String,
    pub first_observed_at: String,
    pub last_observed_at: String,
    pub event_count: i64,
}

pub async fn list_sessions(pool: &SqlitePool, limit: i64) -> Result<Vec<SessionRow>> {
    let rows = sqlx::query(
        "SELECT session_id,
                MIN(observed_at) AS first_observed_at,
                MAX(observed_at) AS last_observed_at,
                COUNT(*)         AS event_count
         FROM observed_event GROUP BY session_id ORDER BY last_observed_at DESC LIMIT ?")
        .bind(limit).fetch_all(pool).await?;
    Ok(rows.into_iter().map(|r| SessionRow {
        session_id: r.get("session_id"),
        first_observed_at: r.get("first_observed_at"),
        last_observed_at: r.get("last_observed_at"),
        event_count: r.get("event_count"),
    }).collect())
}

pub async fn list_session(pool: &SqlitePool, session_id: &str, limit: i64) -> Result<Vec<ObservedEvent>> {
    let rows = sqlx::query(
        "SELECT * FROM observed_event WHERE session_id = ? ORDER BY observed_at ASC LIMIT ?")
        .bind(session_id).bind(limit).fetch_all(pool).await?;
    Ok(rows.into_iter().map(row_to_observed).collect())
}

fn row_to_observed(r: sqlx::sqlite::SqliteRow) -> ObservedEvent {
    let actor: String = r.get("actor");
    let kind: String  = r.get("kind");
    let payload: String = r.get("payload");
    ObservedEvent {
        event_id: r.get("event_id"),
        raw_event_id: r.get("raw_event_id"),
        schema_version: r.get("schema_version"),
        parser_version: r.get("parser_version"),
        session_id: r.get("session_id"),
        event_uuid: r.try_get("event_uuid").ok(),
        parent_uuid: r.try_get("parent_uuid").ok(),
        observed_at: chrono::DateTime::parse_from_rfc3339(&r.get::<String,_>("observed_at"))
                     .unwrap().with_timezone(&chrono::Utc),
        actor: match actor.as_str() {
            "user" => Actor::User, "assistant" => Actor::Assistant,
            "hook" => Actor::Hook, "tool" => Actor::Tool, _ => Actor::System,
        },
        kind: match kind.as_str() {
            "user_message" => EventKind::UserMessage,
            "assistant_message" => EventKind::AssistantMessage,
            "thinking" => EventKind::Thinking,
            "tool_call" => EventKind::ToolCall,
            "tool_result" => EventKind::ToolResult,
            "hook_event" => EventKind::HookEvent,
            "system_summary" => EventKind::SystemSummary,
            "session_state" => EventKind::SessionState,
            "file_history_snapshot" => EventKind::FileHistorySnapshot,
            "attachment_meta" => EventKind::AttachmentMeta,
            _ => EventKind::Unknown,
        },
        subkind: r.try_get("subkind").ok(),
        tool_use_id: r.try_get("tool_use_id").ok(),
        tool_name: r.try_get("tool_name").ok(),
        request_id: r.try_get("request_id").ok(),
        message_id: r.try_get("message_id").ok(),
        turn_id: r.try_get("turn_id").ok(),
        source_tool_assistant_uuid: r.try_get("source_tool_assistant_uuid").ok(),
        source_tool_use_id: r.try_get("source_tool_use_id").ok(),
        is_sidechain: r.get::<i64,_>("is_sidechain") != 0,
        is_meta: r.get::<i64,_>("is_meta") != 0,
        cwd: r.try_get("cwd").ok(),
        git_branch: r.try_get("git_branch").ok(),
        user_type: r.try_get("user_type").ok(),
        entrypoint: r.try_get("entrypoint").ok(),
        cc_version: r.try_get("cc_version").ok(),
        payload: serde_json::from_str(&payload).unwrap_or(serde_json::Value::Null),
    }
}
```

- [ ] **Step 3: Wire `pub mod repo_observed;` in `src/db/mod.rs`**

- [ ] **Step 4: Run test**

Run: `cargo test --test repo_observed`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/db/repo_observed.rs src/db/mod.rs tests/repo_observed.rs
git commit -m "feat(db): repo_observed insert + list_sessions/list_session"
```

---

## Task 12: Ingest store — single-file ingest with txn

**Files:**
- Create: `src/ingest/store.rs`
- Modify: `src/ingest/mod.rs`
- Test: `tests/ingest_store.rs`

- [ ] **Step 1: Write failing test `tests/ingest_store.rs`**

```rust
use sqlx::sqlite::SqlitePoolOptions;
use witmcc::db::{migrate, repo_observed};
use witmcc::ingest::store;

#[tokio::test]
async fn ingest_minimal_fixture_twice_is_idempotent() {
    let pool = SqlitePoolOptions::new().max_connections(2).connect("sqlite::memory:").await.unwrap();
    migrate(&pool).await.unwrap();
    let path = std::path::Path::new("tests/fixtures/transcripts/minimal_session.jsonl");
    let stats1 = store::ingest_file(&pool, path).await.unwrap();
    let stats2 = store::ingest_file(&pool, path).await.unwrap();
    assert!(stats1.observed_inserted > 0);
    assert_eq!(stats2.raw_inserted, 0, "second run inserts no new raw rows");
    let evs = repo_observed::list_session(&pool, "sess-A", 100).await.unwrap();
    // Stable count regardless of how many runs were executed.
    assert_eq!(evs.len(), 6);
}
```

- [ ] **Step 2: Write `src/ingest/store.rs`**

```rust
use std::path::Path;
use chrono::Utc;
use futures::StreamExt;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use serde::Serialize;

use crate::error::{Result, WitmccError};
use crate::ids::MonotonicUlidGen;
use crate::db::{repo_raw, repo_runs, repo_observed};
use crate::ingest::{mapping, transcript};
use crate::model::meta::PARSER_VERSION_TRANSCRIPT;

#[derive(Debug, Default, Serialize)]
pub struct IngestStats {
    pub raw_inserted: u64,
    pub raw_skipped: u64,
    pub observed_inserted: u64,
    pub parse_errors: u64,
    pub sessions_touched: std::collections::BTreeSet<String>,
}

pub async fn ingest_file(pool: &SqlitePool, path: &Path) -> Result<IngestStats> {
    let mut gen = MonotonicUlidGen::new();
    let run_id = repo_runs::start(pool).await?;
    let mut stats = IngestStats::default();
    let mut stream = Box::pin(transcript::stream_file(path).await
        .map_err(|e| WitmccError::Io { path: path.to_path_buf(), source: e })?);

    while let Some(item) = stream.next().await {
        match item {
            Ok((meta, rec)) => {
                let payload_sha = hex::encode(Sha256::digest(&meta.raw));
                let raw_id = gen.next();
                let inserted = repo_raw::insert_dedup(pool, &repo_raw::NewRaw {
                    raw_event_id: raw_id.clone(),
                    ingest_run_id: run_id.clone(),
                    source_type: "claude_transcript".into(),
                    source_uri: meta.source_uri.display().to_string(),
                    source_line_no: meta.line_no as i64,
                    source_byte_offset: meta.byte_offset as i64,
                    payload_sha256: payload_sha,
                    payload: meta.raw.clone(),
                    parse_error: None,
                    captured_at: Utc::now(),
                }).await?;
                if !inserted { stats.raw_skipped += 1; continue; }
                stats.raw_inserted += 1;
                let evs = mapping::map_record(&meta, &rec, &raw_id, &mut gen)?;
                for ev in evs {
                    stats.sessions_touched.insert(ev.session_id.clone());
                    repo_observed::insert(pool, &ev).await?;
                    stats.observed_inserted += 1;
                }
            }
            Err(WitmccError::ParseLine { source_uri, line_no, message }) => {
                stats.parse_errors += 1;
                let raw_id = gen.next();
                let _ = repo_raw::insert_dedup(pool, &repo_raw::NewRaw {
                    raw_event_id: raw_id,
                    ingest_run_id: run_id.clone(),
                    source_type: "unparseable".into(),
                    source_uri,
                    source_line_no: line_no as i64,
                    source_byte_offset: 0,
                    payload_sha256: format!("err-{line_no}"),
                    payload: b"".to_vec(),
                    parse_error: Some(message),
                    captured_at: Utc::now(),
                }).await?;
            }
            Err(other) => return Err(other),
        }
    }

    repo_runs::finish(pool, &run_id, "ok",
        serde_json::to_value(&stats).unwrap_or(serde_json::Value::Null)).await?;
    Ok(stats)
}
```

Wire `pub mod store;` in `src/ingest/mod.rs`.

- [ ] **Step 3: Run test**

Run: `cargo test --test ingest_store`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add src/ingest/store.rs src/ingest/mod.rs tests/ingest_store.rs
git commit -m "feat(ingest): single-file ingest with raw dedup + observed insert"
```

---

## Task 13: Turn-id backfill from `promptId` chain

**Files:**
- Modify: `src/ingest/store.rs`
- Test: `tests/turn_backfill.rs`

- [ ] **Step 1: Write failing test `tests/turn_backfill.rs`**

```rust
use witmcc::db::{migrate, repo_observed};
use witmcc::ingest::store;

#[tokio::test]
async fn backfills_turn_id_for_assistant_in_minimal_session() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1)
        .connect("sqlite::memory:").await.unwrap();
    migrate(&pool).await.unwrap();
    store::ingest_file(&pool, std::path::Path::new("tests/fixtures/transcripts/minimal_session.jsonl")).await.unwrap();
    let evs = repo_observed::list_session(&pool, "sess-A", 100).await.unwrap();
    // The assistant_message with parent u1 should inherit turn_id="p1".
    let assistant = evs.iter().find(|e| e.event_uuid.as_deref() == Some("a1") && matches!(e.kind, witmcc::model::observed::EventKind::AssistantMessage)).unwrap();
    assert_eq!(assistant.turn_id.as_deref(), Some("p1"));
}
```

- [ ] **Step 2: Implement `backfill_turn_ids` in `src/ingest/store.rs`**

Add at bottom:

```rust
pub async fn backfill_turn_ids(pool: &SqlitePool, session_id: &str) -> Result<u64> {
    // Walk parent_uuid chains in memory; cheap enough for slice-1 single-session sizes.
    let rows: Vec<(String, Option<String>, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT event_uuid, parent_uuid, turn_id, event_id
         FROM observed_event WHERE session_id = ? AND event_uuid IS NOT NULL")
        .bind(session_id).fetch_all(pool).await?;
    use std::collections::HashMap;
    let parent_of: HashMap<String, Option<String>> =
        rows.iter().filter_map(|(uuid, parent, _t, _eid)| Some((uuid.clone(), parent.clone()))).collect();
    let prompt_of: HashMap<String, String> = {
        let r: Vec<(String, Option<String>)> = sqlx::query_as(
            "SELECT event_uuid, turn_id FROM observed_event
             WHERE session_id = ? AND event_uuid IS NOT NULL AND turn_id IS NOT NULL")
            .bind(session_id).fetch_all(pool).await?;
        r.into_iter().filter_map(|(u, p)| p.map(|p| (u, p))).collect()
    };

    let mut updates: Vec<(String, String)> = Vec::new(); // (event_id, turn_id)
    for (uuid, _parent, turn_id, event_id) in &rows {
        if turn_id.is_some() { continue; }
        let mut cur = parent_of.get(uuid).cloned().flatten();
        let mut found: Option<String> = None;
        let mut hops = 0usize;
        while let Some(p) = cur {
            if let Some(pid) = prompt_of.get(&p) { found = Some(pid.clone()); break; }
            cur = parent_of.get(&p).cloned().flatten();
            hops += 1;
            if hops > 256 { break; } // cycle guard
        }
        if let Some(tid) = found { updates.push((event_id.clone().unwrap_or_default(), tid)); }
    }

    let mut tx = pool.begin().await?;
    let mut applied = 0u64;
    for (event_id, turn_id) in updates {
        sqlx::query("UPDATE observed_event SET turn_id = ? WHERE event_id = ?")
            .bind(&turn_id).bind(&event_id).execute(&mut *tx).await?;
        applied += 1;
    }
    tx.commit().await?;
    Ok(applied)
}
```

Call it at the end of `ingest_file`, after the loop:

```rust
for session_id in &stats.sessions_touched {
    backfill_turn_ids(pool, session_id).await?;
}
```

(`query_as` for the rows tuple needs the fourth element renamed — adjust the SELECT to `event_uuid, parent_uuid, turn_id, event_id` exactly, which matches.)

- [ ] **Step 3: Run tests**

Run: `cargo test --test turn_backfill && cargo test --test ingest_store`
Expected: both PASS.

- [ ] **Step 4: Commit**

```bash
git add src/ingest/store.rs tests/turn_backfill.rs
git commit -m "feat(ingest): backfill turn_id via parent_uuid->promptId chain"
```

---

## Task 14: Graph builder — `rebuild_session`

**Files:**
- Create: `src/graph/mod.rs`, `src/graph/build.rs`
- Create: `src/db/repo_graph.rs`
- Modify: `src/db/mod.rs`, `src/lib.rs`
- Test: `tests/graph_build.rs`

- [ ] **Step 1: Write failing test `tests/graph_build.rs`**

```rust
use sqlx::sqlite::SqlitePoolOptions;
use witmcc::db::{migrate, repo_graph};
use witmcc::ingest::store;
use witmcc::graph::build;

#[tokio::test]
async fn deterministic_minimal_graph() {
    let pool = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
    migrate(&pool).await.unwrap();
    store::ingest_file(&pool, std::path::Path::new("tests/fixtures/transcripts/minimal_session.jsonl")).await.unwrap();
    build::rebuild_session(&pool, "sess-A").await.unwrap();
    let (n1, e1) = repo_graph::load_session(&pool, "sess-A").await.unwrap();

    // Re-run to verify identical ids/contents.
    build::rebuild_session(&pool, "sess-A").await.unwrap();
    let (n2, e2) = repo_graph::load_session(&pool, "sess-A").await.unwrap();

    let ids = |ns: &[witmcc::model::graph::GraphNode]| -> Vec<String> { ns.iter().map(|n| n.node_id.clone()).collect() };
    let eids = |es: &[witmcc::model::graph::GraphEdge]| -> Vec<String> { es.iter().map(|e| e.edge_id.clone()).collect() };
    assert_eq!(ids(&n1), ids(&n2));
    assert_eq!(eids(&e1), eids(&e2));

    // Spot-check edge kinds present.
    let kinds: std::collections::BTreeSet<_> = e1.iter().map(|e| e.edge_kind.clone()).collect();
    for k in ["turn_order","message_reply","tool_call_to_result"] {
        assert!(kinds.contains(k), "missing edge kind: {k}, got {kinds:?}");
    }
}
```

- [ ] **Step 2: Write `src/db/repo_graph.rs`**

```rust
use sqlx::{SqlitePool, Row};
use crate::error::Result;
use crate::model::graph::{GraphNode, GraphEdge};

pub async fn delete_session(pool: &SqlitePool, session_id: &str) -> Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM graph_edge WHERE session_id = ?").bind(session_id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM graph_node WHERE session_id = ?").bind(session_id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn insert_nodes_edges(pool: &SqlitePool, nodes: &[GraphNode], edges: &[GraphEdge]) -> Result<()> {
    let mut tx = pool.begin().await?;
    for n in nodes {
        sqlx::query("INSERT INTO graph_node(node_id, schema_version, session_id, node_kind, started_at, ended_at, merge_keys, source_event_ids, source_uris, payload) VALUES (?,?,?,?,?,?,?,?,?,?)")
            .bind(&n.node_id).bind(&n.schema_version).bind(&n.session_id).bind(&n.node_kind)
            .bind(n.started_at.to_rfc3339()).bind(n.ended_at.map(|t| t.to_rfc3339()))
            .bind(n.merge_keys.to_string()).bind(serde_json::to_string(&n.source_event_ids).unwrap())
            .bind(serde_json::to_string(&n.source_uris).unwrap()).bind(n.payload.to_string())
            .execute(&mut *tx).await?;
    }
    for e in edges {
        sqlx::query("INSERT INTO graph_edge(edge_id, schema_version, session_id, from_node_id, to_node_id, edge_kind, origin, attributes) VALUES (?,?,?,?,?,?,?,?)")
            .bind(&e.edge_id).bind(&e.schema_version).bind(&e.session_id)
            .bind(&e.from_node_id).bind(&e.to_node_id).bind(&e.edge_kind)
            .bind(&e.origin).bind(e.attributes.to_string())
            .execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn load_session(pool: &SqlitePool, session_id: &str) -> Result<(Vec<GraphNode>, Vec<GraphEdge>)> {
    let nrows = sqlx::query("SELECT * FROM graph_node WHERE session_id = ? ORDER BY started_at ASC, node_id ASC").bind(session_id).fetch_all(pool).await?;
    let erows = sqlx::query("SELECT * FROM graph_edge WHERE session_id = ? ORDER BY edge_id ASC").bind(session_id).fetch_all(pool).await?;
    let nodes = nrows.into_iter().map(|r| GraphNode {
        node_id: r.get("node_id"), schema_version: r.get("schema_version"),
        session_id: r.get("session_id"), node_kind: r.get("node_kind"),
        started_at: chrono::DateTime::parse_from_rfc3339(&r.get::<String,_>("started_at")).unwrap().with_timezone(&chrono::Utc),
        ended_at: r.try_get::<String,_>("ended_at").ok()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok().map(|t| t.with_timezone(&chrono::Utc))),
        merge_keys: serde_json::from_str(&r.get::<String,_>("merge_keys")).unwrap_or(serde_json::Value::Null),
        source_event_ids: serde_json::from_str(&r.get::<String,_>("source_event_ids")).unwrap_or_default(),
        source_uris: serde_json::from_str(&r.get::<String,_>("source_uris")).unwrap_or_default(),
        payload: serde_json::from_str(&r.get::<String,_>("payload")).unwrap_or(serde_json::Value::Null),
    }).collect();
    let edges = erows.into_iter().map(|r| GraphEdge {
        edge_id: r.get("edge_id"), schema_version: r.get("schema_version"),
        session_id: r.get("session_id"),
        from_node_id: r.get("from_node_id"), to_node_id: r.get("to_node_id"),
        edge_kind: r.get("edge_kind"), origin: r.get("origin"),
        attributes: serde_json::from_str(&r.get::<String,_>("attributes")).unwrap_or(serde_json::Value::Null),
    }).collect();
    Ok((nodes, edges))
}
```

Wire `pub mod repo_graph;` in `src/db/mod.rs`.

- [ ] **Step 3: Write `src/graph/mod.rs` and `src/graph/build.rs`**

`src/graph/mod.rs`:

```rust
pub mod build;
```

`src/graph/build.rs`:

```rust
use std::collections::HashMap;
use serde_json::{json, Value};
use sqlx::SqlitePool;

use crate::db::{repo_graph, repo_observed};
use crate::error::Result;
use crate::ids::{derive_edge_id, derive_node_id};
use crate::model::graph::{GraphEdge, GraphNode};
use crate::model::meta::SCHEMA_VERSION;
use crate::model::observed::{EventKind, ObservedEvent};

pub async fn rebuild_session(pool: &SqlitePool, session_id: &str) -> Result<(usize, usize)> {
    let evs = repo_observed::list_session(pool, session_id, 100_000).await?;
    let (nodes, edges) = compute(session_id, &evs);
    repo_graph::delete_session(pool, session_id).await?;
    let n = nodes.len(); let e = edges.len();
    repo_graph::insert_nodes_edges(pool, &nodes, &edges).await?;
    Ok((n, e))
}

pub fn compute(session_id: &str, events: &[ObservedEvent]) -> (Vec<GraphNode>, Vec<GraphEdge>) {
    let mut nodes: Vec<GraphNode> = Vec::new();
    let mut by_event_uuid: HashMap<String, String> = HashMap::new();   // event_uuid -> node_id
    let mut tool_call_node: HashMap<String, usize> = HashMap::new();   // tool_use_id -> nodes index

    // 1. Node materialization
    for e in events {
        let (kind, merge_keys) = match e.kind {
            EventKind::UserMessage      => ("user_message",      json!({"session_id": session_id, "event_uuid": e.event_uuid})),
            EventKind::AssistantMessage => ("assistant_message", json!({"session_id": session_id, "event_uuid": e.event_uuid})),
            EventKind::ToolCall         => ("tool_call",         json!({"session_id": session_id, "tool_use_id": e.tool_use_id})),
            EventKind::ToolResult       => ("tool_result",       json!({"session_id": session_id, "tool_use_id": e.tool_use_id})),
            EventKind::HookEvent        => ("hook_event",        json!({"session_id": session_id, "event_uuid": e.event_uuid})),
            _ => continue, // attachment_meta, session_state, file_history_snapshot, thinking, system_summary, unknown
        };
        let mk_string = merge_keys.to_string();
        let keys_for_hash = canonical_pairs(&merge_keys);
        let node_id = derive_node_id(kind, &keys_for_hash.iter().map(|(k,v)| (k.as_str(), v.as_str())).collect::<Vec<_>>());
        if let Some(uuid) = &e.event_uuid { by_event_uuid.insert(uuid.clone(), node_id.clone()); }
        if kind == "tool_call" {
            if let Some(tid) = &e.tool_use_id { tool_call_node.insert(tid.clone(), nodes.len()); }
        }
        nodes.push(GraphNode {
            node_id,
            schema_version: SCHEMA_VERSION.into(),
            session_id: session_id.into(),
            node_kind: kind.into(),
            started_at: e.observed_at,
            ended_at: None,
            merge_keys: serde_json::from_str(&mk_string).unwrap_or(merge_keys),
            source_event_ids: vec![e.event_id.clone()],
            source_uris: vec![],
            payload: e.payload.clone(),
        });
    }

    // 2. Merge tool_result payload into matching tool_call node (no extra node, no edge).
    //    Dangling tool_result keeps its own node — edge target = tool_call_to_result.
    let mut to_remove: Vec<usize> = Vec::new();
    for (idx, n) in nodes.iter().enumerate() {
        if n.node_kind != "tool_result" { continue; }
        let tid = n.merge_keys.get("tool_use_id").and_then(|x| x.as_str()).map(String::from);
        if let Some(tid) = tid {
            if let Some(call_idx) = tool_call_node.get(&tid).copied() {
                // merge result into call payload
                let mut call_payload = nodes[call_idx].payload.clone();
                if !call_payload.is_object() { call_payload = json!({}); }
                call_payload.as_object_mut().unwrap().insert("result".into(), n.payload.clone());
                nodes[call_idx].payload = call_payload;
                nodes[call_idx].source_event_ids.extend(n.source_event_ids.clone());
                to_remove.push(idx);
            }
        }
    }
    for idx in to_remove.into_iter().rev() { nodes.remove(idx); }

    // 3. Edges
    // 3a. message_reply via parent_uuid
    let mut edges: Vec<GraphEdge> = Vec::new();
    for e in events {
        let Some(child_uuid) = &e.event_uuid else { continue };
        let Some(parent_uuid) = &e.parent_uuid else { continue };
        let (Some(child), Some(parent)) = (by_event_uuid.get(child_uuid), by_event_uuid.get(parent_uuid)) else { continue };
        let attrs = if e.is_sidechain { json!({"crosses_sidechain": true}) } else { json!({}) };
        edges.push(make_edge(session_id, parent, child, "message_reply", attrs));
    }
    // 3b. tool_call_to_result edges only for dangling tool_results
    for n in &nodes {
        if n.node_kind != "tool_result" { continue; }
        let tid = n.merge_keys.get("tool_use_id").and_then(|x| x.as_str()).map(String::from);
        if let Some(tid) = tid {
            // find matching tool_call node (still present if no merge happened)
            if let Some(call_node) = nodes.iter().find(|m| m.node_kind == "tool_call" && m.merge_keys.get("tool_use_id").and_then(|x| x.as_str()) == Some(&tid)) {
                edges.push(make_edge(session_id, &call_node.node_id, &n.node_id, "tool_call_to_result", json!({"matched_via":"tool_use_id"})));
            }
        }
    }
    // 3c. turn_order — adjacent pairs of nodes ordered by (started_at, node_id)
    let mut ordered: Vec<&GraphNode> = nodes.iter().collect();
    ordered.sort_by(|a,b| (a.started_at, &a.node_id).cmp(&(b.started_at, &b.node_id)));
    for w in ordered.windows(2) {
        edges.push(make_edge(session_id, &w[0].node_id, &w[1].node_id, "turn_order", json!({})));
    }

    // Stable output ordering
    nodes.sort_by(|a,b| (a.started_at, &a.node_id).cmp(&(b.started_at, &b.node_id)));
    edges.sort_by(|a,b| a.edge_id.cmp(&b.edge_id));
    (nodes, edges)
}

fn make_edge(session_id: &str, from: &str, to: &str, kind: &str, attrs: Value) -> GraphEdge {
    GraphEdge {
        edge_id: derive_edge_id(from, to, kind),
        schema_version: SCHEMA_VERSION.into(),
        session_id: session_id.into(),
        from_node_id: from.into(),
        to_node_id: to.into(),
        edge_kind: kind.into(),
        origin: "deterministic".into(),
        attributes: attrs,
    }
}

fn canonical_pairs(v: &Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if let Some(map) = v.as_object() {
        for (k, vv) in map { out.push((k.clone(), value_to_string(vv))); }
        out.sort_by(|a,b| a.0.cmp(&b.0));
    }
    out
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::Null => "".into(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}
```

Add `pub mod graph;` in `src/lib.rs`.

- [ ] **Step 4: Run test**

Run: `cargo test --test graph_build`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/graph src/db/repo_graph.rs src/db/mod.rs src/lib.rs tests/graph_build.rs
git commit -m "feat(graph): deterministic rebuild_session (3 edge kinds, hash-derived ids)"
```

---

## Task 15: Wire `ingest` subcommand end-to-end

**Files:**
- Modify: `src/main.rs`
- Create: `src/paths.rs`
- Modify: `src/lib.rs`
- Test: `tests/cli_ingest.rs`

- [ ] **Step 1: Write `src/paths.rs`**

```rust
use std::path::PathBuf;

pub fn default_transcripts_root() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".claude").join("projects"))
}
```

Add `pub mod paths;` in `src/lib.rs`.

- [ ] **Step 2: Write failing test `tests/cli_ingest.rs`**

```rust
use assert_cmd::Command;
use tempfile::tempdir;

#[test]
fn ingest_minimal_fixture_via_cli() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("witmcc.sqlite");
    Command::cargo_bin("witmcc").unwrap()
        .args(["--db-path", db.to_str().unwrap(), "init-db"])
        .assert().success();
    Command::cargo_bin("witmcc").unwrap()
        .args(["--db-path", db.to_str().unwrap(),
               "ingest", "--path", "tests/fixtures/transcripts/minimal_session.jsonl"])
        .assert().success();
    // sanity: file exists and has rows
    let n: i64 = rusqlite_count(&db, "SELECT count(*) FROM observed_event");
    assert!(n >= 6, "got {n}");
    let g: i64 = rusqlite_count(&db, "SELECT count(*) FROM graph_node");
    assert!(g >= 4, "got {g}");
}

fn rusqlite_count(path: &std::path::Path, sql: &str) -> i64 {
    // tiny shim: use sqlite3 CLI to avoid adding rusqlite as dep
    let out = std::process::Command::new("sqlite3").arg(path).arg(sql).output().unwrap();
    String::from_utf8(out.stdout).unwrap().trim().parse().unwrap()
}
```

- [ ] **Step 3: Wire ingest in `src/main.rs`**

Replace the runtime block in `main`:

```rust
rt.block_on(async move {
    match cli.command {
        cli::Command::InitDb => init_db(&cli.db_path).await,
        cli::Command::Ingest { path, all } => ingest_cmd(&cli.db_path, path, all).await,
        cli::Command::Serve  { .. } => Ok(()), // Task 17
    }
})
```

Add helpers:

```rust
async fn ingest_cmd(db_path: &std::path::Path, path: Option<std::path::PathBuf>, all: bool) -> error::Result<()> {
    let url = format!("sqlite://{}?mode=rwc", db_path.display());
    let pool = db::connect(&url).await?;
    db::migrate(&pool).await?;
    let files = collect_files(path, all)?;
    if files.is_empty() {
        tracing::warn!("no JSONL files to ingest");
        return Ok(());
    }
    for f in files {
        tracing::info!(?f, "ingesting");
        let stats = witmcc::ingest::store::ingest_file(&pool, &f).await?;
        tracing::info!(?stats, "ingest done");
        for sid in &stats.sessions_touched {
            let g = witmcc::graph::build::rebuild_session(&pool, sid).await?;
            tracing::info!(session_id=%sid, nodes=g.0, edges=g.1, "graph rebuilt");
        }
    }
    Ok(())
}

fn collect_files(path: Option<std::path::PathBuf>, all: bool) -> error::Result<Vec<std::path::PathBuf>> {
    if let Some(p) = path {
        if p.is_file() { Ok(vec![p]) }
        else if p.is_dir() { Ok(walk_jsonl(&p)) }
        else { Err(error::WitmccError::Invalid(format!("not found: {}", p.display()))) }
    } else if all {
        let root = paths::default_transcripts_root()
            .ok_or_else(|| error::WitmccError::Invalid("HOME not set".into()))?;
        Ok(walk_jsonl(&root))
    } else {
        Err(error::WitmccError::Invalid("provide --path or --all".into()))
    }
}

fn walk_jsonl(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    walkdir::WalkDir::new(root).into_iter().filter_map(|r| r.ok())
        .filter(|e| e.file_type().is_file()
                 && e.path().extension().and_then(|x| x.to_str()) == Some("jsonl"))
        .map(|e| e.into_path()).collect()
}
```

- [ ] **Step 4: Run test**

Run: `cargo test --test cli_ingest`
Expected: PASS. (If `sqlite3` CLI isn't installed, replace the shim with a small sqlx query in the test using `tokio::main`.)

- [ ] **Step 5: Commit**

```bash
git add src/main.rs src/paths.rs src/lib.rs tests/cli_ingest.rs
git commit -m "feat(cli): wire ingest subcommand (path/all, rebuild_session per session)"
```

---

## Task 16: API DTOs + envelope + 4 routes

**Files:**
- Create: `src/api/mod.rs`, `dto.rs`, `routes.rs`, `middleware.rs`
- Modify: `src/lib.rs`
- Test: `tests/api.rs`

- [ ] **Step 1: Write failing test `tests/api.rs`**

```rust
use axum_test::TestServer;
use serde_json::Value;
use sqlx::sqlite::SqlitePoolOptions;
use witmcc::db::migrate;
use witmcc::ingest::store;
use witmcc::graph::build;

async fn setup() -> TestServer {
    let pool = SqlitePoolOptions::new().max_connections(2).connect("sqlite::memory:").await.unwrap();
    migrate(&pool).await.unwrap();
    store::ingest_file(&pool, std::path::Path::new("tests/fixtures/transcripts/minimal_session.jsonl")).await.unwrap();
    build::rebuild_session(&pool, "sess-A").await.unwrap();
    let app = witmcc::api::router(pool);
    TestServer::new(app).unwrap()
}

#[tokio::test]
async fn health() {
    let s = setup().await;
    let resp = s.get("/v1/health").await;
    resp.assert_status_ok();
    let v: Value = resp.json();
    assert_eq!(v["status"], "ok");
}

#[tokio::test]
async fn sessions_list_contains_sess_a() {
    let s = setup().await;
    let v: Value = s.get("/v1/sessions").await.json();
    assert_eq!(v["meta"]["schema_version"], "0.1.0");
    assert!(v["data"].as_array().unwrap().iter().any(|s| s["session_id"]=="sess-A"));
}

#[tokio::test]
async fn session_detail_and_graph() {
    let s = setup().await;
    let detail: Value = s.get("/v1/sessions/sess-A").await.json();
    assert!(detail["data"]["summary"]["event_count"].as_i64().unwrap() >= 6);
    let graph: Value = s.get("/v1/sessions/sess-A/graph").await.json();
    let nodes = graph["data"]["nodes"].as_array().unwrap();
    let edges = graph["data"]["edges"].as_array().unwrap();
    assert!(!nodes.is_empty()); assert!(!edges.is_empty());
}

#[tokio::test]
async fn missing_session_is_404() {
    let s = setup().await;
    s.get("/v1/sessions/missing/graph").await.assert_status(axum::http::StatusCode::NOT_FOUND);
}
```

- [ ] **Step 2: Write `src/api/dto.rs`**

```rust
use serde::Serialize;
use serde_json::Value;

#[derive(Serialize)]
pub struct SessionListItem {
    pub session_id: String,
    pub first_observed_at: String,
    pub last_observed_at: String,
    pub event_count: i64,
    pub source_uris: Vec<String>, // slice-1: empty array (tracked at observed_event payload level)
}

#[derive(Serialize)]
pub struct SessionDetail {
    pub session_id: String,
    pub summary: SessionSummary,
    pub events: Vec<Value>,
}

#[derive(Serialize)]
pub struct SessionSummary {
    pub event_count: i64,
    pub by_kind: std::collections::BTreeMap<String, i64>,
    pub first_observed_at: String,
    pub last_observed_at: String,
}

#[derive(Serialize)]
pub struct GraphPayload {
    pub nodes: Vec<Value>,
    pub edges: Vec<Value>,
}
```

- [ ] **Step 3: Write `src/api/middleware.rs`**

```rust
use axum::{extract::Request, http::StatusCode, middleware::Next, response::Response};

pub async fn host_allowlist(req: Request, next: Next) -> Result<Response, StatusCode> {
    if let Some(host) = req.headers().get(axum::http::header::HOST).and_then(|v| v.to_str().ok()) {
        let bare = host.split(':').next().unwrap_or("");
        if matches!(bare, "127.0.0.1" | "localhost") { return Ok(next.run(req).await); }
    }
    Err(StatusCode::BAD_REQUEST)
}
```

- [ ] **Step 4: Write `src/api/routes.rs`**

```rust
use axum::{extract::{Path, State, Query}, http::StatusCode, response::IntoResponse, Json};
use serde::Deserialize;
use serde_json::json;
use sqlx::SqlitePool;

use crate::api::dto::*;
use crate::db::{repo_graph, repo_observed};
use crate::model::meta::{Envelope, ResponseMeta};

#[derive(Deserialize)]
pub struct ListQuery { pub limit: Option<i64> }

pub async fn health() -> impl IntoResponse {
    Json(json!({"status":"ok","build_sha": option_env!("GIT_SHA").unwrap_or("dev")}))
}

pub async fn list_sessions(State(pool): State<SqlitePool>, Query(q): Query<ListQuery>) -> impl IntoResponse {
    let limit = clamp_limit(q.limit);
    let rows = repo_observed::list_sessions(&pool, limit).await.expect("db");
    let data: Vec<SessionListItem> = rows.into_iter().map(|r| SessionListItem {
        session_id: r.session_id,
        first_observed_at: r.first_observed_at,
        last_observed_at: r.last_observed_at,
        event_count: r.event_count,
        source_uris: vec![],
    }).collect();
    Json(Envelope { meta: ResponseMeta::now(), data })
}

pub async fn session_detail(State(pool): State<SqlitePool>, Path(id): Path<String>, Query(q): Query<ListQuery>)
    -> Result<Json<Envelope<SessionDetail>>, (StatusCode, Json<serde_json::Value>)>
{
    let limit = clamp_limit(q.limit);
    let evs = repo_observed::list_session(&pool, &id, limit).await.expect("db");
    if evs.is_empty() {
        return Err((StatusCode::NOT_FOUND, Json(json!({"type":"about:blank","title":"RESOURCE_NOT_FOUND","detail":format!("session {id} not found")}))));
    }
    let mut by_kind = std::collections::BTreeMap::new();
    for e in &evs { *by_kind.entry(e.kind.as_str().to_string()).or_insert(0) += 1; }
    let first = evs.first().unwrap().observed_at.to_rfc3339();
    let last = evs.last().unwrap().observed_at.to_rfc3339();
    let events: Vec<serde_json::Value> = evs.iter().map(|e| serde_json::to_value(observed_to_dto(e)).unwrap()).collect();
    Ok(Json(Envelope {
        meta: ResponseMeta::now(),
        data: SessionDetail {
            session_id: id,
            summary: SessionSummary { event_count: events.len() as i64, by_kind, first_observed_at: first, last_observed_at: last },
            events,
        },
    }))
}

pub async fn session_graph(State(pool): State<SqlitePool>, Path(id): Path<String>)
    -> Result<Json<Envelope<GraphPayload>>, (StatusCode, Json<serde_json::Value>)>
{
    let (nodes, edges) = repo_graph::load_session(&pool, &id).await.expect("db");
    if nodes.is_empty() {
        return Err((StatusCode::NOT_FOUND, Json(json!({"type":"about:blank","title":"RESOURCE_NOT_FOUND","detail":format!("session {id} has no graph")}))));
    }
    Ok(Json(Envelope {
        meta: ResponseMeta::now(),
        data: GraphPayload {
            nodes: nodes.iter().map(|n| serde_json::to_value(n).unwrap()).collect(),
            edges: edges.iter().map(|e| serde_json::to_value(e).unwrap()).collect(),
        },
    }))
}

fn clamp_limit(l: Option<i64>) -> i64 {
    let v = l.unwrap_or(500);
    v.clamp(1, 5000)
}

// Avoid coupling model::observed to serde details by hand-projecting.
fn observed_to_dto(e: &crate::model::observed::ObservedEvent) -> serde_json::Value {
    json!({
        "event_id": e.event_id,
        "raw_event_id": e.raw_event_id,
        "session_id": e.session_id,
        "event_uuid": e.event_uuid,
        "parent_uuid": e.parent_uuid,
        "observed_at": e.observed_at.to_rfc3339(),
        "actor": e.actor.as_str(),
        "kind": e.kind.as_str(),
        "subkind": e.subkind,
        "tool_use_id": e.tool_use_id,
        "tool_name": e.tool_name,
        "turn_id": e.turn_id,
        "is_sidechain": e.is_sidechain,
        "is_meta": e.is_meta,
        "payload": e.payload,
    })
}
```

- [ ] **Step 5: Write `src/api/mod.rs`**

```rust
pub mod dto;
pub mod middleware;
pub mod routes;

use axum::{routing::get, Router, middleware as axum_mw};
use sqlx::SqlitePool;

pub fn router(pool: SqlitePool) -> Router {
    Router::new()
        .route("/v1/health", get(routes::health))
        .route("/v1/sessions", get(routes::list_sessions))
        .route("/v1/sessions/:id", get(routes::session_detail))
        .route("/v1/sessions/:id/graph", get(routes::session_graph))
        .layer(axum_mw::from_fn(middleware::host_allowlist))
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(pool)
}
```

Add `pub mod api;` in `src/lib.rs`.

- [ ] **Step 6: Run tests**

Run: `cargo test --test api`
Expected: PASS. (axum-test sets `Host: localhost` by default, so middleware allows it.)

- [ ] **Step 7: Commit**

```bash
git add src/api src/lib.rs tests/api.rs
git commit -m "feat(api): GET /v1/health|sessions|sessions/:id|graph + host allowlist"
```

---

## Task 17: Wire `serve` subcommand + bind enforcement

**Files:**
- Modify: `src/main.rs`
- Test: `tests/cli_serve.rs`

- [ ] **Step 1: Write failing test `tests/cli_serve.rs`**

```rust
use std::time::Duration;

#[tokio::test]
async fn serve_returns_health_ok() {
    // Set up DB
    let db = tempfile::NamedTempFile::new().unwrap().into_temp_path();
    assert_cmd::Command::cargo_bin("witmcc").unwrap()
        .args(["--db-path", db.to_str().unwrap(), "init-db"]).assert().success();

    // Spawn serve on port 0? clap accepts u16; we'll pick a high random port.
    let port: u16 = portpicker::pick_unused_port().expect("port");
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_witmcc"))
        .args(["--db-path", db.to_str().unwrap(), "serve",
               "--bind", "127.0.0.1", "--port", &port.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn().unwrap();

    // Wait up to 5s for the server to come up.
    let url = format!("http://127.0.0.1:{port}/v1/health");
    let mut ok = false;
    for _ in 0..50 {
        if reqwest::get(&url).await.is_ok() { ok = true; break; }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let _ = child.kill();
    assert!(ok, "server did not come up at {url}");
}
```

Add `portpicker = "0.1"` and `reqwest = { version = "0.12", default-features = false, features = ["json","rustls-tls"] }` to `[dev-dependencies]`.

- [ ] **Step 2: Implement `serve_cmd` in `src/main.rs`**

```rust
async fn serve_cmd(db_path: &std::path::Path, bind: &str, port: u16, auto_migrate: bool) -> error::Result<()> {
    if bind != "127.0.0.1" {
        return Err(error::WitmccError::Invalid(format!("only 127.0.0.1 is allowed (got {bind})")));
    }
    let url = format!("sqlite://{}?mode=rwc", db_path.display());
    let pool = db::connect(&url).await?;
    if auto_migrate {
        db::migrate(&pool).await?;
    } else {
        // Refuse to serve against an unmigrated DB. Cheap probe: does the
        // primary table exist? (A full migration-state check is post-MVP.)
        let exists: (i64,) = sqlx::query_as(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='observed_event'"
        ).fetch_one(&pool).await?;
        if exists.0 == 0 {
            return Err(error::WitmccError::Invalid(
                "DB has not been migrated; run `witmcc init-db` or pass --auto-migrate".into()));
        }
    }
    let app = witmcc::api::router(pool);
    let addr: std::net::SocketAddr = format!("{bind}:{port}").parse().unwrap();
    tracing::info!(%addr, "serving");
    let listener = tokio::net::TcpListener::bind(addr).await.map_err(anyhow::Error::from)?;
    axum::serve(listener, app).await.map_err(anyhow::Error::from)?;
    Ok(())
}
```

Wire it in the dispatch:

```rust
cli::Command::Serve { bind, port, auto_migrate } => serve_cmd(&cli.db_path, &bind, port, auto_migrate).await,
```

- [ ] **Step 3: Run test**

Run: `cargo test --test cli_serve -- --nocapture`
Expected: PASS within 5 s.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml src/main.rs tests/cli_serve.rs
git commit -m "feat(cli): wire serve subcommand (127.0.0.1 only, --auto-migrate)"
```

---

## Task 18: Determinism regression on golden fixture

**Files:**
- Create: `tests/fixtures/transcripts/dangling_tool_use.jsonl`
- Create: `tests/fixtures/transcripts/sidechain.jsonl`
- Create: `tests/determinism.rs`

- [ ] **Step 1: Add fixtures**

`tests/fixtures/transcripts/dangling_tool_use.jsonl`:

```jsonl
{"type":"user","uuid":"u1","parentUuid":null,"sessionId":"sess-D","timestamp":"2026-05-19T03:00:00Z","cwd":"/","gitBranch":"main","entrypoint":"cli","userType":"external","version":"2.1.144","isSidechain":false,"promptId":"p1","message":{"role":"user","content":"do it"}}
{"type":"assistant","uuid":"a1","parentUuid":"u1","sessionId":"sess-D","timestamp":"2026-05-19T03:00:01Z","cwd":"/","gitBranch":"main","entrypoint":"cli","userType":"external","version":"2.1.144","isSidechain":false,"requestId":"req_d","message":{"id":"msg_d","model":"x","type":"message","role":"assistant","stop_reason":"tool_use","content":[{"type":"tool_use","id":"toolu_dangling","name":"Bash","input":{"command":"ls"}}]}}
```

`tests/fixtures/transcripts/sidechain.jsonl`:

```jsonl
{"type":"user","uuid":"u1","parentUuid":null,"sessionId":"sess-S","timestamp":"2026-05-19T03:00:00Z","cwd":"/","gitBranch":"main","entrypoint":"cli","userType":"external","version":"2.1.144","isSidechain":false,"promptId":"p1","message":{"role":"user","content":"start"}}
{"type":"assistant","uuid":"a1","parentUuid":"u1","sessionId":"sess-S","timestamp":"2026-05-19T03:00:01Z","cwd":"/","gitBranch":"main","entrypoint":"cli","userType":"external","version":"2.1.144","isSidechain":false,"requestId":"req1","message":{"id":"msg1","model":"x","type":"message","role":"assistant","stop_reason":"end_turn","content":[{"type":"text","text":"main"}]}}
{"type":"assistant","uuid":"a2","parentUuid":"a1","sessionId":"sess-S","timestamp":"2026-05-19T03:00:02Z","cwd":"/","gitBranch":"main","entrypoint":"cli","userType":"external","version":"2.1.144","isSidechain":true,"requestId":"req2","message":{"id":"msg2","model":"x","type":"message","role":"assistant","stop_reason":"end_turn","content":[{"type":"text","text":"sidechain"}]}}
```

- [ ] **Step 2: Write `tests/determinism.rs`**

```rust
use sqlx::sqlite::SqlitePoolOptions;
use witmcc::db::migrate;
use witmcc::ingest::store;
use witmcc::graph::build;

async fn ingest_twice(path: &str, session_id: &str) -> (Vec<String>, Vec<String>, Vec<String>, Vec<String>) {
    let pool_a = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
    migrate(&pool_a).await.unwrap();
    store::ingest_file(&pool_a, std::path::Path::new(path)).await.unwrap();
    build::rebuild_session(&pool_a, session_id).await.unwrap();
    let (n_a, e_a) = witmcc::db::repo_graph::load_session(&pool_a, session_id).await.unwrap();

    let pool_b = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
    migrate(&pool_b).await.unwrap();
    store::ingest_file(&pool_b, std::path::Path::new(path)).await.unwrap();
    build::rebuild_session(&pool_b, session_id).await.unwrap();
    let (n_b, e_b) = witmcc::db::repo_graph::load_session(&pool_b, session_id).await.unwrap();

    let ids = |v: &[witmcc::model::graph::GraphNode]| v.iter().map(|x| x.node_id.clone()).collect::<Vec<_>>();
    let eids= |v: &[witmcc::model::graph::GraphEdge]| v.iter().map(|x| x.edge_id.clone()).collect::<Vec<_>>();
    (ids(&n_a), ids(&n_b), eids(&e_a), eids(&e_b))
}

#[tokio::test]
async fn minimal_session_ids_stable_across_databases() {
    let (na, nb, ea, eb) = ingest_twice("tests/fixtures/transcripts/minimal_session.jsonl","sess-A").await;
    pretty_assertions::assert_eq!(na, nb);
    pretty_assertions::assert_eq!(ea, eb);
}

#[tokio::test]
async fn dangling_tool_use_creates_separate_call_node_no_result_edge() {
    let pool = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
    migrate(&pool).await.unwrap();
    store::ingest_file(&pool, std::path::Path::new("tests/fixtures/transcripts/dangling_tool_use.jsonl")).await.unwrap();
    build::rebuild_session(&pool, "sess-D").await.unwrap();
    let (nodes, edges) = witmcc::db::repo_graph::load_session(&pool, "sess-D").await.unwrap();
    assert!(nodes.iter().any(|n| n.node_kind == "tool_call"));
    assert!(!edges.iter().any(|e| e.edge_kind == "tool_call_to_result"));
}

#[tokio::test]
async fn sidechain_edge_is_marked() {
    let pool = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
    migrate(&pool).await.unwrap();
    store::ingest_file(&pool, std::path::Path::new("tests/fixtures/transcripts/sidechain.jsonl")).await.unwrap();
    build::rebuild_session(&pool, "sess-S").await.unwrap();
    let (_, edges) = witmcc::db::repo_graph::load_session(&pool, "sess-S").await.unwrap();
    let crossing = edges.iter().find(|e| e.edge_kind == "message_reply" && e.to_node_id.starts_with("nd_") &&
        e.attributes.get("crosses_sidechain") == Some(&serde_json::Value::Bool(true)));
    assert!(crossing.is_some(), "expected a message_reply edge flagged crosses_sidechain");
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test --test determinism`
Expected: 3 PASS.

- [ ] **Step 4: Commit**

```bash
git add tests/fixtures/transcripts/dangling_tool_use.jsonl tests/fixtures/transcripts/sidechain.jsonl tests/determinism.rs
git commit -m "test(graph): determinism + dangling + sidechain regression suite"
```

---

## Task 19: README + implementation notes

**Files:**
- Create: `README.md`
- Modify: `docs/superpowers/specs/2026-05-19-witmcc-slice1-transcript-design.md` (only if a deviation was needed)

- [ ] **Step 1: Write `README.md`**

```markdown
# witmcc — What's in My Claude Code (slice-1)

Local-only inspection of Claude Code execution. **Slice-1 ships:** transcript
JSONL ingest → SQLite → deterministic-edge session graph → 127.0.0.1 read-only Pull API.

Out of slice-1 (later slices): OTel/Hook/File-Git collectors, UI, MCP, redaction, auth.

## Quick start

```bash
cargo run -- init-db
cargo run -- ingest --all                     # scans ~/.claude/projects/**/*.jsonl
cargo run -- serve                            # 127.0.0.1:7878
curl http://127.0.0.1:7878/v1/health
curl http://127.0.0.1:7878/v1/sessions | jq .
```

## Endpoints

| GET path | response |
| --- | --- |
| `/v1/health` | `{status, build_sha}` |
| `/v1/sessions` | list of `{session_id, first_observed_at, last_observed_at, event_count}` |
| `/v1/sessions/{id}` | `{summary, events[]}` |
| `/v1/sessions/{id}/graph` | `{nodes[], edges[]}` |

All non-health responses are wrapped in `{meta: {schema_version, collection_profile, generated_at, ...}, data: ...}`.

## Tests

```bash
cargo test
```

## Known limits in slice-1

- No redaction. Do not point at JSONL files that may contain secrets you're unwilling to expose to anything that can reach 127.0.0.1.
- No live tail. Re-run `ingest` to pick up newly appended JSONL lines (idempotent).
- `tool_call_to_result` edges only appear for *dangling* tool_results — matched results are merged into the `tool_call` node payload.
- `last-prompt`, `permission-mode`, `file-history-snapshot`, `thinking`, and non-hook `attachment` events are preserved as ObservedEvents but do not get their own graph nodes.

## Reference docs

- Spec (this slice): `docs/superpowers/specs/2026-05-19-witmcc-slice1-transcript-design.md`
- Plan: `docs/superpowers/plans/2026-05-19-witmcc-slice1-transcript.md`
- Full system docs: `docs/index.html` and `docs/00..06_*.html`
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: slice-1 README"
```

---

## Final Verification

- [ ] **Step 1: Full test sweep**

Run: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`
Expected: all GREEN.

- [ ] **Step 2: End-to-end smoke against real data (optional, no commit)**

```bash
cargo run -- --db-path /tmp/witmcc-real.sqlite init-db
cargo run -- --db-path /tmp/witmcc-real.sqlite ingest --all
cargo run -- --db-path /tmp/witmcc-real.sqlite serve &
SERVER=$!
sleep 1
curl -s http://127.0.0.1:7878/v1/sessions | jq '.data | length'
kill $SERVER
rm /tmp/witmcc-real.sqlite*
```

Expected: a session list with one or more entries, including
`3a07124f-9ec0-4282-a625-4c3849494376`.

- [ ] **Step 3: Confirm spec coverage**

Re-read `docs/superpowers/specs/2026-05-19-witmcc-slice1-transcript-design.md` and confirm every section maps to at least one task above. If anything is uncovered, file a follow-up task before declaring slice-1 done.

---

## Spec Coverage Map (self-review)

| Spec section | Tasks |
|---|---|
| Module Layout | 1, 2, 6, 7, 8, 9, 10, 11, 12, 14, 16 |
| SQLite Schema | 5, 6 |
| ObservedEvent Mapping | 9 |
| Splitting `assistant` content | 9 |
| Turn ID backfill | 13 |
| Deterministic Graph Builder (3 edge kinds, hash-derived ids) | 4, 14 |
| Idempotency | 10, 12, 18 |
| Pull API (4 endpoints + envelope + pagination clamp) | 16 |
| Middleware (Host allowlist, 127.0.0.1 only) | 16, 17 |
| Error Handling (parse errors, unknown type, panics) | 9, 12, 16 |
| CLI (init-db / ingest / serve, flags) | 2, 6, 15, 17 |
| Testing Strategy | 2, 6, 8, 9, 10, 11, 12, 13, 14, 16, 17, 18 |
| Forward-Compatibility Locks | 5, 7 |

Open Questions in the spec are intentionally not covered — they belong to later slices.
