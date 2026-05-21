//! Background git poller (slice-5).
//!
//! Opens the repository at startup, remembers the current `HEAD` OID as the
//! "last-seen tip", then on every tick walks any new commits between the old
//! tip (exclusive) and the new tip (inclusive) in topological order. Each new
//! commit is persisted via [`crate::ingest::file_git::store_commit`]. Honours a
//! `CancellationToken` for graceful shutdown.

use crate::ingest::file_git::{extract_commit_records, store_commit};
use chrono::Utc;
use git2::Repository;
use std::path::PathBuf;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

use crate::ingest::file_git::{CommitRecord, HunkRecord};

type ExtractedBatch = (Option<String>, Vec<(CommitRecord, Vec<HunkRecord>)>);

pub async fn run_git_poller(
    pool: sqlx::SqlitePool,
    repo_path: PathBuf,
    interval_secs: u64,
    cancel: CancellationToken,
) -> anyhow::Result<()> {
    // Initialise last-seen tip without entering the async context (git2 is !Send).
    let init = {
        let p = repo_path.clone();
        tokio::task::spawn_blocking(move || -> Option<String> {
            match Repository::open(&p) {
                Ok(r) => r
                    .head()
                    .and_then(|h| h.peel_to_commit())
                    .ok()
                    .map(|c| c.id().to_string()),
                Err(_) => None,
            }
        })
        .await
        .ok()
        .flatten()
    };
    let mut last_seen: Option<String> = init;

    let mut ticker = tokio::time::interval(Duration::from_secs(interval_secs.max(1)));
    ticker.tick().await; // burn the immediate tick

    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            _ = ticker.tick() => {
                let last = last_seen.clone();
                let p = repo_path.clone();
                let extracted = tokio::task::spawn_blocking(move || extract_new(&p, last)).await;
                match extracted {
                    Ok(Ok((new_tip, batch))) => {
                        for (commit, hunks) in batch {
                            if let Err(e) = store_commit(&pool, commit, hunks, Utc::now(), &crate::live::NoopSink).await {
                                tracing::warn!(error=?e, "store_commit failed");
                            }
                        }
                        if new_tip.is_some() {
                            last_seen = new_tip;
                        }
                    }
                    Ok(Err(e)) => {
                        tracing::warn!(error=?e, "git poll iteration failed");
                    }
                    Err(e) => {
                        tracing::warn!(error=?e, "git poll join failed");
                    }
                }
            }
        }
    }
    Ok(())
}

fn extract_new(
    repo_path: &std::path::Path,
    last_seen: Option<String>,
) -> anyhow::Result<ExtractedBatch> {
    let repo = Repository::open(repo_path)?;
    let head_oid = match repo.head().and_then(|h| h.peel_to_commit()) {
        Ok(c) => c.id(),
        Err(_) => return Ok((None, Vec::new())),
    };

    if Some(head_oid.to_string()) == last_seen {
        return Ok((Some(head_oid.to_string()), Vec::new()));
    }

    let mut walk = repo.revwalk()?;
    walk.push(head_oid)?;
    if let Some(prev_str) = last_seen.as_ref() {
        if let Ok(prev_oid) = git2::Oid::from_str(prev_str) {
            let _ = walk.hide(prev_oid);
        }
    }
    walk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::REVERSE)?;

    let mut batch: Vec<(CommitRecord, Vec<HunkRecord>)> = Vec::new();
    for oid_res in walk {
        let oid = oid_res?;
        let commit = repo.find_commit(oid)?;
        let (cr, hr) = extract_commit_records(&repo, &commit)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        batch.push((cr, hr));
    }
    Ok((Some(head_oid.to_string()), batch))
}
