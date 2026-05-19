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
