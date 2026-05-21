//! Slice-7 — live tail for `~/.claude/projects/**/*.jsonl`.
//!
//! Design spec proposed an in-memory byte-offset cursor; the actual
//! implementation re-runs `ingest::store::ingest_file` on each debounced
//! flush and lets the existing `(source_uri, source_line_no, payload_sha256)`
//! UNIQUE constraint absorb the dedup. The trade-off is documented in
//! `docs/implementation-notes.html` (DEV-S7-01): every flush rehashes the
//! whole file, which is bounded by Claude Code's per-session JSONL size
//! (typically MB-scale). In exchange we get zero cursor-management state to
//! corrupt, and a single code path with `witmcc ingest --all`.
//!
//! Failure semantics mirror slice-5 file watcher: fail-soft, never panic,
//! never poison the serve process. Cancellation via `CancellationToken`.

use crate::ingest::store;
use crate::live::{BroadcastSink, LiveEvent};
use anyhow::Result;
use notify::{Event, EventKind, RecursiveMode, Watcher};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc};
use tokio_util::sync::CancellationToken;

const DEBOUNCE_MS: u64 = 100;
const TICK_MS: u64 = 50;

fn is_jsonl(p: &Path) -> bool {
    p.extension().and_then(|x| x.to_str()) == Some("jsonl")
}

/// Run the transcript live tail until `cancel` is triggered.
///
/// On startup the function performs a one-shot ingest of every `.jsonl` file
/// under `root` (catch-up for sessions that ran before serve started). Then
/// it watches `root` recursively; debounced notify events trigger an
/// `ingest_file` call per touched path.
pub async fn run(
    pool: SqlitePool,
    root: PathBuf,
    live_tx: Arc<broadcast::Sender<LiveEvent>>,
    cancel: CancellationToken,
) -> Result<()> {
    if !root.exists() {
        tracing::warn!(?root, "transcripts root does not exist; live tail disabled");
        return Ok(());
    }
    tracing::info!(?root, "transcript live tail started");

    let sink = BroadcastSink::new(live_tx.clone());

    // Initial catch-up scan runs in the background so serve startup is not
    // blocked when the user's transcripts root contains large session files.
    // The notify watcher below picks up live writes regardless of whether the
    // catch-up has finished. Dedup via UNIQUE constraint means the two paths
    // cannot insert duplicates if they race on the same file.
    {
        let pool_cl = pool.clone();
        let root_cl = root.clone();
        let cancel_cl = cancel.clone();
        let sink_cl = sink.clone();
        tokio::spawn(async move {
            tokio::select! {
                _ = cancel_cl.cancelled() => {}
                res = scan_initial(&pool_cl, &root_cl, &sink_cl) => {
                    if let Some((files, inserted)) = res {
                        tracing::info!(files, observed_inserted = inserted, "initial transcript scan complete");
                    }
                }
            }
        });
    }

    let (tx, mut rx) = mpsc::unbounded_channel::<PathBuf>();
    let tx_for_watcher = tx.clone();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
        if let Ok(ev) = res {
            if matches!(
                ev.kind,
                EventKind::Modify(_) | EventKind::Create(_) | EventKind::Any
            ) {
                for p in ev.paths {
                    if is_jsonl(&p) {
                        let _ = tx_for_watcher.send(p);
                    }
                }
            }
        }
    })?;
    watcher.watch(&root, RecursiveMode::Recursive)?;

    let mut pending: HashMap<PathBuf, Instant> = HashMap::new();
    let mut ticker = tokio::time::interval(Duration::from_millis(TICK_MS));

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                tracing::info!("transcript tail shutting down");
                break;
            }
            maybe_p = rx.recv() => {
                match maybe_p {
                    Some(p) => { pending.insert(p, Instant::now()); }
                    None => break, // watcher dropped
                }
            }
            _ = ticker.tick() => {
                let now = Instant::now();
                let ready: Vec<PathBuf> = pending
                    .iter()
                    .filter(|(_, t)| now.duration_since(**t).as_millis() >= DEBOUNCE_MS as u128)
                    .map(|(p, _)| p.clone())
                    .collect();
                for p in &ready {
                    pending.remove(p);
                    if !p.exists() {
                        continue;
                    }
                    match store::ingest_file(&pool, p, &sink).await {
                        Ok(stats) => {
                            if stats.observed_inserted > 0 {
                                tracing::debug!(
                                    path = %p.display(),
                                    observed_inserted = stats.observed_inserted,
                                    "transcript tail ingested"
                                );
                            }
                        }
                        Err(e) => {
                            tracing::warn!(error = ?e, path = %p.display(), "transcript tail ingest failed");
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

async fn scan_initial(
    pool: &SqlitePool,
    root: &Path,
    sink: &BroadcastSink,
) -> Option<(usize, u64)> {
    let mut files = 0usize;
    let mut total_inserted = 0u64;
    for entry in walkdir::WalkDir::new(root).into_iter().flatten() {
        let p = entry.path();
        if !p.is_file() || !is_jsonl(p) {
            continue;
        }
        files += 1;
        match store::ingest_file(pool, p, sink).await {
            Ok(s) => total_inserted += s.observed_inserted,
            Err(e) => {
                tracing::warn!(error = ?e, path = %p.display(), "initial transcript scan failed");
            }
        }
    }
    if files == 0 {
        None
    } else {
        Some((files, total_inserted))
    }
}
