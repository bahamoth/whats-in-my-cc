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
