use clap::Parser;
use witmcc::{cli, db, doctor, error, insight::judge::runtime::JudgeRuntime, paths, telemetry};

fn main() -> error::Result<()> {
    let cli = cli::Cli::parse();
    telemetry::init(&cli.log_format, cli.verbose);
    let rt = tokio::runtime::Runtime::new().map_err(anyhow::Error::from)?;
    rt.block_on(async move {
        match cli.command {
            cli::Command::InitDb => init_db(&cli.db_path).await,
            cli::Command::Doctor {
                json,
                server,
                project,
            } => {
                let code = doctor::run(doctor::DoctorOpts {
                    json,
                    server,
                    project,
                })
                .await
                .map_err(anyhow::Error::from)?;
                std::process::exit(code);
            }
            cli::Command::Ingest { path, all } => ingest_cmd(&cli.db_path, path, all).await,
            cli::Command::Serve {
                bind,
                port,
                auto_migrate,
                shutdown_after_ms,
                no_watch_transcripts,
                transcripts_root,
                sse_keepalive_secs,
                sse_channel_capacity,
                judge,
                judge_budget,
                judge_fixture_path,
            } => {
                serve_cmd(
                    &cli.db_path,
                    bind,
                    port,
                    auto_migrate,
                    shutdown_after_ms,
                    no_watch_transcripts,
                    transcripts_root,
                    sse_keepalive_secs,
                    sse_channel_capacity,
                    judge,
                    judge_budget,
                    judge_fixture_path,
                )
                .await
            }
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

#[allow(clippy::too_many_arguments)]
async fn serve_cmd(
    db_path: &std::path::Path,
    bind: std::net::IpAddr,
    port: u16,
    auto_migrate: bool,
    shutdown_after_ms: Option<u64>,
    no_watch_transcripts: bool,
    transcripts_root: Option<std::path::PathBuf>,
    sse_keepalive_secs: u64,
    sse_channel_capacity: u64,
    judge_mode: cli::JudgeMode,
    judge_budget: usize,
    judge_fixture_path: Option<std::path::PathBuf>,
) -> error::Result<()> {
    // Loopback-only enforcement: accepts 127.0.0.0/8 and ::1 (is_loopback()).
    // Strict 127.0.0.1-only would use `bind == IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)`.
    // Slice-1 uses is_loopback() to also allow ::1 for IPv6 loopback.
    if !bind.is_loopback() {
        return Err(error::WitmccError::Invalid(format!(
            "only loopback addresses are allowed (got {bind})"
        )));
    }
    let url = format!("sqlite://{}?mode=rwc", db_path.display());
    let pool = db::connect(&url).await?;
    if auto_migrate {
        db::migrate(&pool).await?;
    } else {
        // Refuse to serve against an unmigrated DB. Cheap probe: does the
        // primary table exist?
        let exists: (i64,) = sqlx::query_as(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='observed_event'",
        )
        .fetch_one(&pool)
        .await?;
        if exists.0 == 0 {
            return Err(error::WitmccError::Invalid(
                "DB has not been migrated; run `witmcc init-db` or pass --auto-migrate".into(),
            ));
        }
    }
    let cancel = tokio_util::sync::CancellationToken::new();
    let mut bg_handles: Vec<tokio::task::JoinHandle<()>> = Vec::new();

    let (live_tx, _) =
        tokio::sync::broadcast::channel::<witmcc::live::LiveEvent>(sse_channel_capacity as usize);
    let live_tx = std::sync::Arc::new(live_tx);

    if !no_watch_transcripts {
        let root = transcripts_root
            .clone()
            .or_else(paths::default_transcripts_root);
        match root {
            Some(r) => {
                tracing::info!(root = ?r, "transcript live tail enabled");
                let pool_cl = pool.clone();
                let tok = cancel.clone();
                let live_cl = live_tx.clone();
                bg_handles.push(tokio::spawn(async move {
                    if let Err(e) = witmcc::transcript_tail::run(pool_cl, r, live_cl, tok).await {
                        tracing::error!(error=?e, "transcript tail exited with error");
                    }
                }));
            }
            None => {
                tracing::warn!(
                    "no transcripts root found; pass --transcripts-root or disable with --no-watch-transcripts"
                );
            }
        }
    }

    if let Some(ms) = shutdown_after_ms {
        let tok = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
            tok.cancel();
        });
    }

    // Slice-15: build judge runtime based on CLI flags.
    let judge_runtime = match judge_mode {
        cli::JudgeMode::None => JudgeRuntime::noop(),
        cli::JudgeMode::Fixture => {
            let path = judge_fixture_path.ok_or_else(|| {
                error::WitmccError::Invalid(
                    "--judge-fixture-path is required when --judge=fixture".into(),
                )
            })?;
            JudgeRuntime::fixture(&path, judge_budget).map_err(anyhow::Error::from)?
        }
        cli::JudgeMode::Anthropic => {
            JudgeRuntime::anthropic(pool.clone(), judge_budget).map_err(anyhow::Error::from)?
        }
    };
    tracing::info!(judge = %judge_runtime.kind, budget = judge_runtime.budget, "judge runtime ready");

    let state = witmcc::api::AppState {
        pool: pool.clone(),
        live_tx: live_tx.clone(),
        sse_keepalive_secs,
        sse_channel_capacity: sse_channel_capacity as usize,
        judge_runtime: std::sync::Arc::new(judge_runtime),
        // Slice-17: MCP session registry starts empty; sessions are created on initialize.
        mcp_sessions: witmcc::api::mcp::SessionRegistry::new(),
    };
    let app = witmcc::api::router(state);
    let addr = std::net::SocketAddr::new(bind, port);
    tracing::info!(%addr, "serving");
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(anyhow::Error::from)?;
    let shutdown_signal = {
        let tok = cancel.clone();
        async move {
            tokio::select! {
                _ = tok.cancelled() => {}
                _ = tokio::signal::ctrl_c() => {}
            }
        }
    };
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal)
        .await
        .map_err(anyhow::Error::from)?;
    cancel.cancel();
    for h in bg_handles {
        let _ = h.await;
    }
    Ok(())
}

async fn ingest_cmd(
    db_path: &std::path::Path,
    path: Option<std::path::PathBuf>,
    all: bool,
) -> error::Result<()> {
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
        let stats = witmcc::ingest::store::ingest_file(&pool, &f, &witmcc::live::NoopSink).await?;
        tracing::info!(?stats, "ingest done");
        for sid in &stats.sessions_touched {
            let g = witmcc::graph::build::rebuild_session(&pool, sid).await?;
            tracing::info!(session_id=%sid, nodes=g.0, edges=g.1, "graph rebuilt");
        }
    }
    Ok(())
}

fn collect_files(
    path: Option<std::path::PathBuf>,
    all: bool,
) -> error::Result<Vec<std::path::PathBuf>> {
    if let Some(p) = path {
        if p.is_file() {
            Ok(vec![p])
        } else if p.is_dir() {
            Ok(walk_jsonl(&p))
        } else {
            Err(error::WitmccError::Invalid(format!(
                "not found: {}",
                p.display()
            )))
        }
    } else if all {
        let root = paths::default_transcripts_root()
            .ok_or_else(|| error::WitmccError::Invalid("HOME not set".into()))?;
        Ok(walk_jsonl(&root))
    } else {
        Err(error::WitmccError::Invalid(
            "provide --path or --all".into(),
        ))
    }
}

fn walk_jsonl(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(|r| r.ok())
        .filter(|e| {
            e.file_type().is_file()
                && e.path().extension().and_then(|x| x.to_str()) == Some("jsonl")
        })
        .map(|e| e.into_path())
        .collect()
}
