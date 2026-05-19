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
