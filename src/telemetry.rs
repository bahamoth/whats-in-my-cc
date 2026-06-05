use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use crate::cli::LogFormat;

pub fn init(format: &LogFormat, verbose: bool) {
    let default_level = if verbose { "debug" } else { "info" };
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(format!("wimcc={default_level},sqlx=warn,axum=info")));

    let reg = tracing_subscriber::registry().with(filter);
    match format {
        LogFormat::Pretty => reg.with(fmt::layer().with_target(false)).init(),
        LogFormat::Json => reg.with(fmt::layer().json().with_target(false)).init(),
    }
}
