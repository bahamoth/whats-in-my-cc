use clap::Parser;
use witmcc::{cli, db, error, paths, telemetry};

fn main() -> error::Result<()> {
    let cli = cli::Cli::parse();
    telemetry::init(&cli.log_format, cli.verbose);
    let rt = tokio::runtime::Runtime::new().map_err(anyhow::Error::from)?;
    rt.block_on(async move {
        match cli.command {
            cli::Command::InitDb => init_db(&cli.db_path).await,
            cli::Command::Ingest { path, all } => ingest_cmd(&cli.db_path, path, all).await,
            cli::Command::Serve  { .. } => Ok(()), // Task 17
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
