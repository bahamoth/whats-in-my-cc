mod cli;
mod error;
mod ids;
mod telemetry;

use clap::Parser;

fn main() -> error::Result<()> {
    let cli = cli::Cli::parse();
    telemetry::init(&cli.log_format, cli.verbose);
    tracing::info!(?cli.command, "witmcc starting");
    Ok(())
}
