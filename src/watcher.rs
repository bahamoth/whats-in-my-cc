//! Background filesystem watcher (slice-5).
//!
//! Wraps `notify::recommended_watcher` with a 250 ms in-memory debounce so a
//! rapid create/unlink/modify sequence on the same `(path, change_type)`
//! coalesces into a single `file_event`. Honours a `CancellationToken` for
//! graceful shutdown alongside the HTTP server.

use crate::ingest::file_git::{self, FileChange, FileRecord, FILESYSTEM_SESSION_ID};
use chrono::Utc;
use notify::{
    event::{CreateKind, ModifyKind, RemoveKind, RenameMode},
    EventKind, RecommendedWatcher, RecursiveMode, Watcher,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

const DEBOUNCE: Duration = Duration::from_millis(250);
const POLL_TICK: Duration = Duration::from_millis(100);

pub async fn run_file_watcher(
    pool: sqlx::SqlitePool,
    root: PathBuf,
    cancel: CancellationToken,
) -> anyhow::Result<()> {
    let (tx, mut rx) = mpsc::unbounded_channel::<RawFsEvent>();
    let root_for_filter = root.clone();
    let mut watcher: RecommendedWatcher = notify::recommended_watcher({
        let tx = tx.clone();
        let root = root_for_filter.clone();
        move |res: notify::Result<notify::Event>| {
            if let Ok(event) = res {
                for raw in classify(&event, &root) {
                    let _ = tx.send(raw);
                }
            }
        }
    })?;
    watcher.watch(&root, RecursiveMode::Recursive)?;
    drop(tx); // only the closure keeps a sender

    let mut pending: HashMap<(PathBuf, FileChange), (Instant, Option<PathBuf>)> = HashMap::new();
    let mut ticker = tokio::time::interval(POLL_TICK);

    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            _ = ticker.tick() => {
                let now = Instant::now();
                let mut due: Vec<((PathBuf, FileChange), Option<PathBuf>)> = Vec::new();
                pending.retain(|k, v| {
                    if now.duration_since(v.0) >= DEBOUNCE {
                        due.push((k.clone(), v.1.clone()));
                        false
                    } else {
                        true
                    }
                });
                for ((path, change_type), old_path) in due {
                    let size_bytes = std::fs::metadata(&path).ok().map(|m| m.len());
                    let record = FileRecord {
                        session_id: FILESYSTEM_SESSION_ID.into(),
                        path: path.to_string_lossy().into_owned(),
                        change_type,
                        old_path: old_path.map(|p| p.to_string_lossy().into_owned()),
                        size_bytes,
                        observed_at: Utc::now(),
                    };
                    if let Err(e) = file_git::store_file_event(&pool, record, Utc::now()).await {
                        tracing::warn!(error=?e, "store_file_event failed");
                    }
                }
            }
            msg = rx.recv() => {
                let Some(raw) = msg else { break };
                let key = (raw.path.clone(), raw.change);
                let entry = pending
                    .entry(key)
                    .or_insert((Instant::now(), None));
                entry.0 = Instant::now();
                if raw.change == FileChange::Renamed {
                    entry.1 = raw.old_path.clone();
                }
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct RawFsEvent {
    path: PathBuf,
    change: FileChange,
    old_path: Option<PathBuf>,
}

fn classify(event: &notify::Event, root: &Path) -> Vec<RawFsEvent> {
    let mut out: Vec<RawFsEvent> = Vec::new();
    let change = match event.kind {
        EventKind::Create(CreateKind::File | CreateKind::Folder | CreateKind::Any | CreateKind::Other) => {
            FileChange::Created
        }
        EventKind::Modify(ModifyKind::Data(_))
        | EventKind::Modify(ModifyKind::Metadata(_))
        | EventKind::Modify(ModifyKind::Any)
        | EventKind::Modify(ModifyKind::Other) => FileChange::Modified,
        EventKind::Modify(ModifyKind::Name(RenameMode::Any))
        | EventKind::Modify(ModifyKind::Name(RenameMode::To))
        | EventKind::Modify(ModifyKind::Name(RenameMode::Both))
        | EventKind::Modify(ModifyKind::Name(RenameMode::From))
        | EventKind::Modify(ModifyKind::Name(RenameMode::Other)) => FileChange::Renamed,
        EventKind::Remove(RemoveKind::File | RemoveKind::Folder | RemoveKind::Any | RemoveKind::Other) => {
            FileChange::Deleted
        }
        _ => return out,
    };
    let (new, old) = match (change, event.paths.as_slice()) {
        (FileChange::Renamed, [from, to]) => (to.clone(), Some(from.clone())),
        (_, paths) if !paths.is_empty() => (paths[0].clone(), None),
        _ => return out,
    };
    if should_ignore(&new, root) {
        return out;
    }
    out.push(RawFsEvent {
        path: new,
        change,
        old_path: old,
    });
    out
}

fn should_ignore(path: &Path, _root: &Path) -> bool {
    // Match `.git` / `target` anywhere in the path components — `strip_prefix`
    // does not handle macOS `/tmp -> /private/tmp` symlink rewriting, so the
    // root-relative check would otherwise miss events on macOS temp paths.
    for comp in path.components() {
        if let std::path::Component::Normal(name) = comp {
            if name == std::ffi::OsStr::new(".git") || name == std::ffi::OsStr::new("target") {
                return true;
            }
        }
    }
    if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
        if name.ends_with(".sqlite")
            || name.ends_with(".sqlite-wal")
            || name.ends_with(".sqlite-shm")
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignore_lists_sqlite_wal_and_git() {
        let root = Path::new("/tmp/r");
        assert!(should_ignore(Path::new("/tmp/r/.git/HEAD"), root));
        assert!(should_ignore(Path::new("/tmp/r/sub/.git/refs"), root));
        assert!(should_ignore(Path::new("/tmp/r/target/debug/x"), root));
        assert!(should_ignore(Path::new("/tmp/r/foo.sqlite"), root));
        assert!(should_ignore(Path::new("/tmp/r/foo.sqlite-wal"), root));
        assert!(!should_ignore(Path::new("/tmp/r/src/main.rs"), root));
    }

    #[test]
    fn ignore_handles_canonical_macos_private_prefix() {
        // macOS resolves /tmp -> /private/tmp, so watcher events arrive with
        // /private/tmp/... even if the user passed /tmp/... as the root.
        let root = Path::new("/tmp/r");
        assert!(should_ignore(Path::new("/private/tmp/r/.git/HEAD"), root));
        assert!(should_ignore(Path::new("/private/tmp/r/target/x"), root));
        assert!(!should_ignore(Path::new("/private/tmp/r/src/main.rs"), root));
    }
}
