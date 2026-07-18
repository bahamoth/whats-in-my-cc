use clap::Parser;
use wimcc::{cli, db, doctor, error, paths, telemetry};

fn main() -> error::Result<()> {
    let cli = cli::Cli::parse();
    // 2026-07-18: default DB path moved from CWD-relative `.wimcc.sqlite` to the
    // platform data dir; a legacy CWD file still wins so existing DBs keep
    // working. Resolve once here so every subcommand agrees on the same path.
    let (db_path, db_source) = paths::resolve_db_path(cli.db_path.clone());
    // Commands that open (or record, for `service install`) the DB. The others
    // (self-update, service uninstall/…) must not create the data dir or log a
    // DB line for a database they never touch. Doctor reports the path in its
    // own output instead.
    let uses_db = matches!(
        &cli.command,
        cli::Command::InitDb
            | cli::Command::Ingest { .. }
            | cli::Command::Serve { .. }
            | cli::Command::Service {
                action: cli::ServiceAction::Install { .. }
            }
    );
    if uses_db && db_source == paths::DbPathSource::DataDir {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(anyhow::Error::from)?;
        }
    }
    // Only a real `serve` (not the --print-token/--rotate-token short-circuits)
    // gets a rotating file log next to its DB; other commands stay console-only.
    let file_log = match &cli.command {
        cli::Command::Serve {
            print_token: false,
            rotate_token: false,
            ..
        } => Some((
            telemetry::resolve_log_dir(&db_path, cli.log_dir.as_deref()),
            cli.log_retention_days,
        )),
        _ => None,
    };
    // The guard must outlive the whole run so the non-blocking writer flushes.
    let _log_guard = telemetry::init(
        &cli.log_format,
        cli.verbose,
        file_log.as_ref().map(|(dir, keep)| (dir.as_path(), *keep)),
    );
    if let Some((dir, keep)) = &file_log {
        let abs = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.clone());
        tracing::info!(dir = %abs.display(), keep_days = keep, "rotating file log enabled");
    }
    if uses_db {
        match db_source {
            paths::DbPathSource::LegacyCwd => tracing::info!(
                path = %db_path.display(),
                "legacy DB in current directory — using it (default location: <data_dir>/wimcc/wimcc.sqlite)"
            ),
            _ => tracing::info!(
                path = %db_path.display(),
                source = db_source.as_str(),
                "db path resolved"
            ),
        }
    }
    let rt = tokio::runtime::Runtime::new().map_err(anyhow::Error::from)?;
    rt.block_on(async move {
        match cli.command {
            cli::Command::InitDb => init_db(&db_path).await,
            cli::Command::Vacuum => vacuum_cmd(&db_path).await,
            cli::Command::Doctor {
                json,
                server,
                project,
            } => {
                let code = doctor::run(doctor::DoctorOpts {
                    json,
                    server,
                    project,
                    db_path: db_path.clone(),
                    db_path_source: db_source.as_str(),
                })
                .await
                .map_err(anyhow::Error::from)?;
                std::process::exit(code);
            }
            cli::Command::Ingest { path, all } => ingest_cmd(&db_path, path, all).await,
            cli::Command::Serve {
                bind,
                port,
                auto_migrate,
                shutdown_after_ms,
                no_watch_transcripts,
                transcripts_root,
                sse_keepalive_secs,
                sse_channel_capacity,
                print_token,
                rotate_token,
                retention_profile,
                auth,
                update_check,
            } => {
                // Slice-19: --print-token and --rotate-token short-circuit server start.
                if print_token {
                    let token = wimcc::security::token::ensure_token()?;
                    eprintln!("{token}");
                    return Ok(());
                }
                if rotate_token {
                    let token = wimcc::security::token::rotate_token()?;
                    eprintln!("{token}");
                    return Ok(());
                }
                serve_cmd(
                    &db_path,
                    bind,
                    port,
                    auto_migrate,
                    shutdown_after_ms,
                    no_watch_transcripts,
                    transcripts_root,
                    sse_keepalive_secs,
                    sse_channel_capacity,
                    retention_profile,
                    auth,
                    update_check,
                )
                .await
            }
            cli::Command::SelfUpdate { check } => {
                wimcc::self_update::run(check).await.map_err(Into::into)
            }
            cli::Command::Service { action } => match action {
                cli::ServiceAction::Install {
                    bind,
                    port,
                    auto_migrate,
                } => wimcc::service::install(&db_path, &bind.to_string(), port, auto_migrate)
                    .map_err(Into::into),
                cli::ServiceAction::Uninstall => wimcc::service::uninstall().map_err(Into::into),
                cli::ServiceAction::Restart => wimcc::service::restart().map_err(Into::into),
                cli::ServiceAction::Status => wimcc::service::status().map_err(Into::into),
            },
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

/// growth-2026-07-18 — one-shot compaction for DBs created before
/// auto_vacuum=INCREMENTAL became the default (their header stays NONE, so
/// the retention sweep's incremental_vacuum is a no-op on them).
async fn vacuum_cmd(path: &std::path::Path) -> error::Result<()> {
    let url = format!("sqlite://{}?mode=rwc", path.display());
    let (before, after) = db::vacuum_db(&url).await?;
    tracing::info!(
        ?path,
        before_bytes = before,
        after_bytes = after,
        "vacuum complete"
    );
    eprintln!("vacuum: {before} -> {after} bytes ({})", path.display());
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
    retention_profile: String,
    auth: cli::AuthMode,
    update_check: String,
) -> error::Result<()> {
    // Loopback-only enforcement: accepts 127.0.0.0/8 and ::1 (is_loopback()).
    // Strict 127.0.0.1-only would use `bind == IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)`.
    // Slice-1 uses is_loopback() to also allow ::1 for IPv6 loopback.
    if !bind.is_loopback() {
        return Err(error::WimccError::Invalid(format!(
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
            return Err(error::WimccError::Invalid(
                "DB has not been migrated; run `wimcc init-db` or pass --auto-migrate".into(),
            ));
        }
    }

    // Dogfooding 2026-06-11: backfill agent_id for rows ingested before migration
    // 0023 (raw payload → agent_id). Idempotent (only NULL rows); new ingests fill
    // it via mapping. Non-fatal — a backfill hiccup must not block serve.
    match db::repo_observed::backfill_agent_id(&pool).await {
        Ok(n) if n > 0 => tracing::info!(rows = n, "backfilled agent_id from raw payload"),
        Ok(_) => {}
        Err(e) => tracing::warn!(error = ?e, "agent_id backfill failed (non-fatal)"),
    }
    // 0024 (file path → workflow_run_id). Deterministic workflow group key, present
    // only in raw_event.source_uri. Idempotent; new ingests fill it via store.rs.
    match db::repo_observed::backfill_workflow_run_id(&pool).await {
        Ok(n) if n > 0 => tracing::info!(rows = n, "backfilled workflow_run_id from source_uri"),
        Ok(_) => {}
        Err(e) => tracing::warn!(error = ?e, "workflow_run_id backfill failed (non-fatal)"),
    }
    // 0026 (raw payload → agent_name/team_name). Teammate 세션 식별 — raw에
    // 원본 envelope 필드가 보존돼 있어 재ingest 없이 복구 가능. Idempotent.
    match db::repo_observed::backfill_team_fields(&pool).await {
        Ok(n) if n > 0 => tracing::info!(rows = n, "backfilled team fields from raw payload"),
        Ok(_) => {}
        Err(e) => tracing::warn!(error = ?e, "team fields backfill failed (non-fatal)"),
    }
    // 0025 (perf-2026-06-29): materialize per-session transcript facets so
    // /v1/sessions reads them instead of re-scanning observed_event. Fills only
    // sessions missing from session_summary; new/updated ones refresh via
    // recompute_session. Non-fatal.
    match db::repo_observed::backfill_session_summary(&pool).await {
        Ok(n) if n > 0 => tracing::info!(sessions = n, "backfilled session_summary facets"),
        Ok(_) => {}
        Err(e) => tracing::warn!(error = ?e, "session_summary backfill failed (non-fatal)"),
    }

    let cancel = tokio_util::sync::CancellationToken::new();
    let mut bg_handles: Vec<tokio::task::JoinHandle<()>> = Vec::new();

    let (live_tx, _) =
        tokio::sync::broadcast::channel::<wimcc::live::LiveEvent>(sse_channel_capacity as usize);
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
                    if let Err(e) = wimcc::transcript_tail::run(pool_cl, r, live_cl, tok).await {
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

    // DEV-S19-08: --auth off (default) → token empty string, middleware bypasses.
    // --auth on → ensure token exists, print on first boot.
    let token = match auth {
        cli::AuthMode::Off => {
            tracing::info!(
                "auth disabled (single-user dev); pass --auth on to enforce bearer token"
            );
            String::new()
        }
        cli::AuthMode::On => {
            let t = wimcc::security::token::ensure_token()?;
            eprintln!("wimcc: serving with token {t}");
            t
        }
    };

    // Slice-19: Parse retention profile and spawn sweep if enabled.
    // growth-2026-07-18: the sweep task reports into shared SweepStats,
    // surfaced by /v1/health.
    let sweep_stats: wimcc::security::retention::SharedSweepStats = Default::default();
    let profile: wimcc::security::retention::Profile = retention_profile.parse()?;
    if profile != wimcc::security::retention::Profile::None {
        let pool_cl = pool.clone();
        let policy = wimcc::security::retention::RetentionPolicy {
            profile: profile.clone(),
        };
        bg_handles.push(wimcc::security::retention::spawn_sweep_task(
            pool_cl,
            policy,
            sweep_stats.clone(),
            cancel.clone(),
        ));
        tracing::info!(profile = retention_profile, "retention sweep enabled");
    }

    // 2026-07-17 §4: background update-check loop. `--update-check off` means
    // zero outbound calls (spec's only outbound path is this check).
    let update_status: wimcc::update_check::SharedUpdateStatus = Default::default();
    if update_check == "on" {
        let url = std::env::var("WIMCC_UPDATE_CHECK_URL")
            .unwrap_or_else(|_| wimcc::update_check::DEFAULT_LATEST_RELEASE_URL.to_string());
        tokio::spawn(wimcc::update_check::run_update_check_loop(
            update_status.clone(),
            url,
            cancel.clone(),
        ));
        tracing::info!("update check loop enabled (24h interval)");
    } else {
        tracing::info!("update check disabled (--update-check off)");
    }

    let state = wimcc::api::AppState {
        pool: pool.clone(),
        live_tx: live_tx.clone(),
        sse_keepalive_secs,
        sse_channel_capacity: sse_channel_capacity as usize,
        // Slice-17: MCP session registry starts empty; sessions are created on initialize.
        mcp_sessions: wimcc::api::mcp::SessionRegistry::new(),
        // Slice-19: bearer token + retention profile for health block.
        token,
        retention_profile,
        // Post-slice-19: same token long-lived stream handlers observe.
        shutdown: cancel.clone(),
        // 2026-07-17 §4: update-check loop writes, health handler reads.
        update_status,
        // growth-2026-07-18: sweep task writes, health handler reads.
        sweep_stats,
        db_path: Some(db_path.display().to_string()),
    };
    let app = wimcc::api::router(state);
    let addr = std::net::SocketAddr::new(bind, port);
    tracing::info!(%addr, "serving");
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(anyhow::Error::from)?;
    let shutdown_signal = wimcc::serve::shutdown_with_grace(cancel.clone());
    let serve_fut = axum::serve(listener, app).with_graceful_shutdown(shutdown_signal);
    // Grace window counts from cancel — never aborts a still-active server.
    wimcc::serve::run_serve_with_grace(
        serve_fut,
        cancel.clone(),
        wimcc::serve::DEFAULT_SHUTDOWN_GRACE,
    )
    .await;
    cancel.cancel();
    // bg_handles join is also bounded by grace; tokens have been cancelled
    // so well-behaved tasks return immediately. anything stuck is dropped.
    let _ = tokio::time::timeout(wimcc::serve::DEFAULT_SHUTDOWN_GRACE, async {
        for h in bg_handles {
            let _ = h.await;
        }
    })
    .await;
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
    // Backfill agent_id on existing rows (raw payload) BEFORE recompute, so the
    // per-session insight pass (re_read scope) sees subagent attribution even on
    // dedup-skipped rows. New rows get it via mapping (dogfooding 2026-06-11).
    match db::repo_observed::backfill_agent_id(&pool).await {
        Ok(n) if n > 0 => tracing::info!(rows = n, "backfilled agent_id from raw payload"),
        Ok(_) => {}
        Err(e) => tracing::warn!(error = ?e, "agent_id backfill failed (non-fatal)"),
    }
    // 0024 (file path → workflow_run_id). Deterministic workflow group key, present
    // only in raw_event.source_uri. Idempotent; new ingests fill it via store.rs.
    match db::repo_observed::backfill_workflow_run_id(&pool).await {
        Ok(n) if n > 0 => tracing::info!(rows = n, "backfilled workflow_run_id from source_uri"),
        Ok(_) => {}
        Err(e) => tracing::warn!(error = ?e, "workflow_run_id backfill failed (non-fatal)"),
    }
    // 0026 (raw payload → agent_name/team_name). Teammate 세션 식별 — raw에
    // 원본 envelope 필드가 보존돼 있어 재ingest 없이 복구 가능. Idempotent.
    match db::repo_observed::backfill_team_fields(&pool).await {
        Ok(n) if n > 0 => tracing::info!(rows = n, "backfilled team fields from raw payload"),
        Ok(_) => {}
        Err(e) => tracing::warn!(error = ?e, "team fields backfill failed (non-fatal)"),
    }
    let files = collect_files(path, all)?;
    if files.is_empty() {
        tracing::warn!("no JSONL files to ingest");
        return Ok(());
    }
    // Batch: ingest every file's raw lines, then recompute each touched session's
    // insights ONCE (not per file). On `--all` over subagent-heavy corpora this
    // avoids recomputing a session dozens of times (dogfooding 2026-06-11).
    tracing::info!(file_count = files.len(), "ingesting (batch)");
    let stats = wimcc::ingest::store::ingest_paths(&pool, &files, &wimcc::live::NoopSink).await?;
    tracing::info!(?stats, "ingest done");
    // 0025 (perf-2026-06-29): backfill facets for any sessions present in the DB
    // but missing from session_summary (e.g. pre-migration rows). Ingested
    // sessions are already refreshed by recompute_session; this catches the rest.
    match db::repo_observed::backfill_session_summary(&pool).await {
        Ok(n) if n > 0 => tracing::info!(sessions = n, "backfilled session_summary facets"),
        Ok(_) => {}
        Err(e) => tracing::warn!(error = ?e, "session_summary backfill failed (non-fatal)"),
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
            Err(error::WimccError::Invalid(format!(
                "not found: {}",
                p.display()
            )))
        }
    } else if all {
        let root = paths::default_transcripts_root()
            .ok_or_else(|| error::WimccError::Invalid("HOME not set".into()))?;
        Ok(walk_jsonl(&root))
    } else {
        Err(error::WimccError::Invalid("provide --path or --all".into()))
    }
}

fn walk_jsonl(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    // *.jsonl + subagent 사이드카 meta.json (호출 관계의 원본) — lib과 공유.
    wimcc::ingest::discover_files(root)
}
