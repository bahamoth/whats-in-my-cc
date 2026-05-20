//! File/Git observer ingest (slice-5).
//!
//! Source-preserving: each filesystem mutation and each git commit/hunk is
//! persisted as one `RawEvent` (`source_type="file_git"`) + one `ObservedEvent`
//! and surfaces on the synthetic session [`FILESYSTEM_SESSION_ID`].

use crate::db::{repo_diff_hunk, repo_observed, repo_raw, repo_runs};
use crate::error::Result;
use crate::ids::MonotonicUlidGen;
use crate::model::meta::{PARSER_VERSION_FILE_GIT, SCHEMA_VERSION};
use crate::model::observed::{Actor, EventKind, ObservedEvent};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::collections::BTreeSet;

pub const FILESYSTEM_SESSION_ID: &str = "filesystem";

/// Per-commit safety cap so a pathological monorepo commit never blows up the
/// observed_event table. Surplus hunks are dropped and counted in
/// [`CommitIngestResult::dropped_hunks_over_limit`].
pub const MAX_HUNKS_PER_COMMIT: usize = 2000;

/// Hunk `patch_preview` is truncated to this many bytes (UTF-8 safe boundary).
pub const PATCH_PREVIEW_MAX_BYTES: usize = 4 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileChange {
    Created,
    Modified,
    Deleted,
    Renamed,
}

impl FileChange {
    pub fn as_str(&self) -> &'static str {
        match self {
            FileChange::Created => "created",
            FileChange::Modified => "modified",
            FileChange::Deleted => "deleted",
            FileChange::Renamed => "renamed",
        }
    }
}

#[derive(Debug, Clone)]
pub struct FileRecord {
    pub session_id: String,
    pub path: String,
    pub change_type: FileChange,
    pub old_path: Option<String>,
    pub size_bytes: Option<u64>,
    pub observed_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct CommitSignature {
    pub name: String,
    pub email: String,
    pub time: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct CommitRecord {
    pub session_id: String,
    pub repo: String,
    pub sha: String,
    pub parents: Vec<String>,
    pub author: CommitSignature,
    pub committer: CommitSignature,
    pub message: String,
    pub branch: Option<String>,
    pub files_changed: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct HunkRecord {
    pub diff_hunk_id: String,
    pub session_id: String,
    pub file_path: String,
    /// "added" | "modified" | "deleted" | "renamed"
    pub change_type: String,
    /// `(start, end)` line range on the post-image side. `None` for binary diffs.
    pub line_range_after: Option<(u32, u32)>,
    pub introduced_by_commit_sha: String,
    pub patch_preview: String,
    pub lines_added: u32,
    pub lines_removed: u32,
}

pub fn file_record_to_payload(r: &FileRecord) -> Value {
    let mut m = serde_json::Map::new();
    m.insert("path".into(), Value::String(r.path.clone()));
    m.insert(
        "change_type".into(),
        Value::String(r.change_type.as_str().into()),
    );
    if let Some(op) = &r.old_path {
        m.insert("old_path".into(), Value::String(op.clone()));
    }
    if let Some(sz) = r.size_bytes {
        m.insert("size_bytes".into(), Value::Number(sz.into()));
    }
    m.insert(
        "observed_at".into(),
        Value::String(r.observed_at.to_rfc3339()),
    );
    json!({ "file": Value::Object(m) })
}

pub fn commit_record_to_payload(r: &CommitRecord) -> Value {
    json!({
        "git": {
            "repo":      r.repo,
            "sha":       r.sha,
            "parents":   r.parents,
            "author":    {
                "name":  r.author.name,
                "email": r.author.email,
                "time":  r.author.time.to_rfc3339(),
            },
            "committer": {
                "name":  r.committer.name,
                "email": r.committer.email,
                "time":  r.committer.time.to_rfc3339(),
            },
            "message":      r.message,
            "branch":       r.branch,
            "files_changed": r.files_changed,
        }
    })
}

pub fn hunk_record_to_payload(r: &HunkRecord) -> Value {
    json!({
        "hunk": {
            "diff_hunk_id":             r.diff_hunk_id,
            "file_path":                r.file_path,
            "change_type":              r.change_type,
            "line_range_after":         r.line_range_after.map(|(a,b)| json!({"start": a, "end": b})),
            "introduced_by_commit_sha": r.introduced_by_commit_sha,
            "patch_preview":            r.patch_preview,
            "lines_added":              r.lines_added,
            "lines_removed":            r.lines_removed,
        }
    })
}

/// Truncate `s` to at most `PATCH_PREVIEW_MAX_BYTES` bytes on a UTF-8 char
/// boundary, appending `\n…[truncated]` if anything was dropped.
pub fn truncate_patch_preview(s: &str) -> String {
    if s.len() <= PATCH_PREVIEW_MAX_BYTES {
        return s.to_string();
    }
    let mut end = PATCH_PREVIEW_MAX_BYTES;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    let mut out = String::with_capacity(end + 16);
    out.push_str(&s[..end]);
    out.push_str("\n…[truncated]");
    out
}

#[derive(Debug, Default, Serialize)]
pub struct FileIngestResult {
    pub accepted_events: u64,
    pub duplicate_events: u64,
    pub sessions_touched: Vec<String>,
}

/// Persist a single filesystem mutation as `raw_event` + `observed_event`.
///
/// Honours the slice-3/4 self-heal pattern: the session is marked touched and
/// re-built even when the raw row deduplicates, so a stale graph recovers on
/// the next event.
pub async fn store_file_event(
    pool: &SqlitePool,
    record: FileRecord,
    received_at: DateTime<Utc>,
) -> Result<FileIngestResult> {
    let mut gen = MonotonicUlidGen::new();
    let run_id = repo_runs::start(pool).await?;
    let mut result = FileIngestResult::default();
    let mut touched: BTreeSet<String> = BTreeSet::new();

    let payload = file_record_to_payload(&record);
    let canonical = canonical_json(&payload);
    let canonical_bytes = canonical.as_bytes().to_vec();
    let payload_sha = hex::encode(Sha256::digest(&canonical_bytes));
    let source_uri = format!("file://{}", record.path);
    let raw_id = gen.generate();

    let inserted = repo_raw::insert_dedup(
        pool,
        &repo_raw::NewRaw {
            raw_event_id: raw_id.clone(),
            ingest_run_id: run_id.clone(),
            source_type: "file_git".into(),
            source_uri,
            source_line_no: 0,
            source_byte_offset: 0,
            payload_sha256: payload_sha,
            payload: canonical_bytes,
            parse_error: None,
            captured_at: received_at,
        },
    )
    .await?;

    // Self-heal (DEV-S3-07): touched is recorded BEFORE the dedup-skip so a
    // re-emit against a stale graph still rebuilds.
    touched.insert(record.session_id.clone());

    if !inserted {
        result.duplicate_events += 1;
    } else {
        let event = ObservedEvent {
            event_id: gen.generate(),
            raw_event_id: raw_id,
            schema_version: SCHEMA_VERSION.into(),
            session_id: record.session_id.clone(),
            observed_at: record.observed_at,
            actor: Actor::System,
            kind: EventKind::FileEvent,
            subkind: Some(record.change_type.as_str().into()),
            payload,
            parser_version: PARSER_VERSION_FILE_GIT.into(),
            ..Default::default()
        };
        repo_observed::insert(pool, &event).await?;
        result.accepted_events += 1;
    }

    for session_id in &touched {
        crate::graph::build::rebuild_session(pool, session_id).await?;
    }

    repo_runs::finish(
        pool,
        &run_id,
        "ok",
        serde_json::to_value(&result).unwrap_or(Value::Null),
    )
    .await?;

    result.sessions_touched = touched.into_iter().collect();
    Ok(result)
}

#[derive(Debug, Default, Serialize)]
pub struct CommitIngestResult {
    pub accepted_commits: u64,
    pub duplicate_commits: u64,
    pub accepted_hunks: u64,
    pub duplicate_hunks: u64,
    pub dropped_hunks_over_limit: u64,
    pub sessions_touched: Vec<String>,
}

/// Persist a commit (one `git_commit` `ObservedEvent`) and its hunks (one
/// `diff_hunk` `ObservedEvent` per hunk + one row in the `diff_hunk` table).
///
/// `hunks` SHOULD already be capped at [`MAX_HUNKS_PER_COMMIT`] by the caller
/// via [`extract_commit_records`]; if `hunks.len()` exceeds that, the surplus
/// is dropped and counted.
pub async fn store_commit(
    pool: &SqlitePool,
    commit: CommitRecord,
    hunks: Vec<HunkRecord>,
    received_at: DateTime<Utc>,
) -> Result<CommitIngestResult> {
    let mut gen = MonotonicUlidGen::new();
    let run_id = repo_runs::start(pool).await?;
    let mut result = CommitIngestResult::default();
    let mut touched: BTreeSet<String> = BTreeSet::new();

    // ---- commit row ----
    let commit_payload = commit_record_to_payload(&commit);
    let commit_canon = canonical_json(&commit_payload);
    let commit_bytes = commit_canon.as_bytes().to_vec();
    let commit_sha = hex::encode(Sha256::digest(&commit_bytes));
    let commit_uri = format!("git://{}/commit/{}", commit.repo, commit.sha);
    let commit_raw_id = gen.generate();

    let commit_inserted = repo_raw::insert_dedup(
        pool,
        &repo_raw::NewRaw {
            raw_event_id: commit_raw_id.clone(),
            ingest_run_id: run_id.clone(),
            source_type: "file_git".into(),
            source_uri: commit_uri,
            source_line_no: 0,
            source_byte_offset: 0,
            payload_sha256: commit_sha,
            payload: commit_bytes,
            parse_error: None,
            captured_at: received_at,
        },
    )
    .await?;

    touched.insert(commit.session_id.clone());

    if commit_inserted {
        let ev = ObservedEvent {
            event_id: gen.generate(),
            raw_event_id: commit_raw_id,
            schema_version: SCHEMA_VERSION.into(),
            session_id: commit.session_id.clone(),
            observed_at: commit.committer.time,
            actor: Actor::System,
            kind: EventKind::GitCommit,
            subkind: Some("commit".into()),
            payload: commit_payload,
            parser_version: PARSER_VERSION_FILE_GIT.into(),
            ..Default::default()
        };
        repo_observed::insert(pool, &ev).await?;
        result.accepted_commits += 1;
    } else {
        result.duplicate_commits += 1;
    }

    // ---- hunks ----
    let (kept, dropped) = if hunks.len() > MAX_HUNKS_PER_COMMIT {
        let drop_count = (hunks.len() - MAX_HUNKS_PER_COMMIT) as u64;
        let mut v = hunks;
        v.truncate(MAX_HUNKS_PER_COMMIT);
        (v, drop_count)
    } else {
        (hunks, 0u64)
    };
    result.dropped_hunks_over_limit = dropped;

    for hunk in kept {
        let hunk_payload = hunk_record_to_payload(&hunk);
        let hunk_canon = canonical_json(&hunk_payload);
        let hunk_bytes = hunk_canon.as_bytes().to_vec();
        let hunk_sha = hex::encode(Sha256::digest(&hunk_bytes));
        let line_repr = match hunk.line_range_after {
            Some((a, b)) => format!("{a}-{b}"),
            None => "binary".into(),
        };
        let hunk_uri = format!(
            "git://{}/commit/{}/hunk/{}:{}",
            commit.repo, commit.sha, hunk.file_path, line_repr
        );
        let hunk_raw_id = gen.generate();

        let inserted = repo_raw::insert_dedup(
            pool,
            &repo_raw::NewRaw {
                raw_event_id: hunk_raw_id.clone(),
                ingest_run_id: run_id.clone(),
                source_type: "file_git".into(),
                source_uri: hunk_uri,
                source_line_no: 0,
                source_byte_offset: 0,
                payload_sha256: hunk_sha,
                payload: hunk_bytes,
                parse_error: None,
                captured_at: received_at,
            },
        )
        .await?;

        if !inserted {
            result.duplicate_hunks += 1;
            continue;
        }

        let event_id = gen.generate();
        let ev = ObservedEvent {
            event_id: event_id.clone(),
            raw_event_id: hunk_raw_id,
            schema_version: SCHEMA_VERSION.into(),
            session_id: hunk.session_id.clone(),
            observed_at: commit.committer.time,
            actor: Actor::System,
            kind: EventKind::DiffHunk,
            subkind: Some(hunk.change_type.clone()),
            payload: hunk_payload,
            parser_version: PARSER_VERSION_FILE_GIT.into(),
            ..Default::default()
        };
        repo_observed::insert(pool, &ev).await?;

        repo_diff_hunk::insert(
            pool,
            &repo_diff_hunk::NewDiffHunk {
                diff_hunk_id: hunk.diff_hunk_id.clone(),
                schema_version: SCHEMA_VERSION.into(),
                session_id: hunk.session_id.clone(),
                file_path: hunk.file_path.clone(),
                change_type: hunk.change_type.clone(),
                line_start_after: hunk.line_range_after.map(|(a, _)| a as i64),
                line_end_after: hunk.line_range_after.map(|(_, b)| b as i64),
                introduced_by_node_id: None,
                related_observed_event_id: Some(event_id),
            },
        )
        .await?;

        result.accepted_hunks += 1;
    }

    for session_id in &touched {
        crate::graph::build::rebuild_session(pool, session_id).await?;
    }

    repo_runs::finish(
        pool,
        &run_id,
        "ok",
        serde_json::to_value(&result).unwrap_or(Value::Null),
    )
    .await?;

    result.sessions_touched = touched.into_iter().collect();
    Ok(result)
}

/// Extract a [`CommitRecord`] and one [`HunkRecord`] per hunk for `commit`.
///
/// `diff_hunk_id` is deterministic across re-runs (`hunk_{sha}_{idx}`) so that
/// re-ingesting the same commit dedupes both at the raw-event layer (via
/// canonical-JSON sha256) and at the `diff_hunk` side-table (via PRIMARY KEY).
pub fn extract_commit_records(
    repo: &git2::Repository,
    commit: &git2::Commit,
) -> Result<(CommitRecord, Vec<HunkRecord>)> {
    use chrono::TimeZone;

    let sha = commit.id().to_string();
    let parents: Vec<String> = commit
        .parent_ids()
        .map(|oid| oid.to_string())
        .collect();

    fn sig_to_record(sig: &git2::Signature) -> CommitSignature {
        let secs = sig.when().seconds();
        let time = chrono::Utc.timestamp_opt(secs, 0).single().unwrap_or_else(Utc::now);
        CommitSignature {
            name: sig.name().unwrap_or("").into(),
            email: sig.email().unwrap_or("").into(),
            time,
        }
    }
    let author = sig_to_record(&commit.author());
    let committer = sig_to_record(&commit.committer());
    let message = commit.message().unwrap_or("").to_string();

    let branch = repo
        .head()
        .ok()
        .and_then(|h| h.shorthand().map(|s| s.to_string()));

    let new_tree = commit.tree().map_err(|e| crate::error::WitmccError::Other(anyhow::Error::from(e)))?;
    let parent_tree = if commit.parent_count() > 0 {
        Some(
            commit
                .parent(0)
                .and_then(|p| p.tree())
                .map_err(|e| crate::error::WitmccError::Other(anyhow::Error::from(e)))?,
        )
    } else {
        None
    };

    let mut diff_opts = git2::DiffOptions::new();
    diff_opts.include_typechange(true);
    let diff = repo
        .diff_tree_to_tree(parent_tree.as_ref(), Some(&new_tree), Some(&mut diff_opts))
        .map_err(|e| crate::error::WitmccError::Other(anyhow::Error::from(e)))?;

    let mut files_changed: Vec<String> = Vec::new();
    let mut hunks: Vec<HunkRecord> = Vec::new();

    let num_deltas = diff.deltas().len();
    for delta_idx in 0..num_deltas {
        let delta = match diff.get_delta(delta_idx) {
            Some(d) => d,
            None => continue,
        };
        let file_path = delta
            .new_file()
            .path()
            .or_else(|| delta.old_file().path())
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        if !file_path.is_empty() {
            files_changed.push(file_path.clone());
        }
        let change_type: &str = match delta.status() {
            git2::Delta::Added => "added",
            git2::Delta::Deleted => "deleted",
            git2::Delta::Renamed | git2::Delta::Copied => "renamed",
            _ => "modified",
        };

        let patch_opt =
            git2::Patch::from_diff(&diff, delta_idx).map_err(|e| crate::error::WitmccError::Other(anyhow::Error::from(e)))?;

        // Binary: emit a single hunk with null line range + "<binary>".
        let is_binary =
            delta.new_file().is_binary() || delta.old_file().is_binary() || patch_opt.is_none();
        if is_binary {
            hunks.push(HunkRecord {
                diff_hunk_id: format!("hunk_{}_b{}", sha, delta_idx),
                session_id: FILESYSTEM_SESSION_ID.into(),
                file_path: file_path.clone(),
                change_type: change_type.into(),
                line_range_after: None,
                introduced_by_commit_sha: sha.clone(),
                patch_preview: "<binary>".into(),
                lines_added: 0,
                lines_removed: 0,
            });
            continue;
        }

        let patch = patch_opt.unwrap();
        let nh = patch.num_hunks();
        for hidx in 0..nh {
            let (hunk_meta, line_count) = patch
                .hunk(hidx)
                .map_err(|e| crate::error::WitmccError::Other(anyhow::Error::from(e)))?;
            let header = std::str::from_utf8(hunk_meta.header()).unwrap_or("").to_string();
            let new_start = hunk_meta.new_start();
            let new_lines = hunk_meta.new_lines();
            let line_range_after = if new_lines == 0 {
                None
            } else {
                Some((new_start, new_start + new_lines - 1))
            };

            let mut preview = String::new();
            preview.push_str(&header);
            let mut added: u32 = 0;
            let mut removed: u32 = 0;
            for lidx in 0..line_count {
                let line = patch
                    .line_in_hunk(hidx, lidx)
                    .map_err(|e| crate::error::WitmccError::Other(anyhow::Error::from(e)))?;
                let origin = line.origin();
                let content = std::str::from_utf8(line.content()).unwrap_or("");
                match origin {
                    '+' => {
                        added += 1;
                        preview.push('+');
                        preview.push_str(content);
                    }
                    '-' => {
                        removed += 1;
                        preview.push('-');
                        preview.push_str(content);
                    }
                    ' ' => {
                        preview.push(' ');
                        preview.push_str(content);
                    }
                    _ => {}
                }
            }

            hunks.push(HunkRecord {
                diff_hunk_id: format!("hunk_{}_{}", sha, hunks.len()),
                session_id: FILESYSTEM_SESSION_ID.into(),
                file_path: file_path.clone(),
                change_type: change_type.into(),
                line_range_after,
                introduced_by_commit_sha: sha.clone(),
                patch_preview: truncate_patch_preview(&preview),
                lines_added: added,
                lines_removed: removed,
            });
        }
    }

    let commit_record = CommitRecord {
        session_id: FILESYSTEM_SESSION_ID.into(),
        repo: repo
            .workdir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| repo.path().to_string_lossy().to_string()),
        sha,
        parents,
        author,
        committer,
        message,
        branch,
        files_changed,
    };

    Ok((commit_record, hunks))
}

pub fn canonical_json(value: &Value) -> String {
    fn norm(v: &Value) -> Value {
        match v {
            Value::Object(map) => {
                let mut keys: Vec<&String> = map.keys().collect();
                keys.sort();
                let mut out = serde_json::Map::new();
                for k in keys {
                    out.insert(k.clone(), norm(&map[k]));
                }
                Value::Object(out)
            }
            Value::Array(arr) => Value::Array(arr.iter().map(norm).collect()),
            _ => v.clone(),
        }
    }
    norm(value).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 5, 20, 12, 0, 0).unwrap()
    }

    fn sample_file_record() -> FileRecord {
        FileRecord {
            session_id: FILESYSTEM_SESSION_ID.into(),
            path: "/tmp/a.rs".into(),
            change_type: FileChange::Modified,
            old_path: None,
            size_bytes: Some(42),
            observed_at: now(),
        }
    }

    fn sample_commit_record() -> CommitRecord {
        CommitRecord {
            session_id: FILESYSTEM_SESSION_ID.into(),
            repo: "/tmp/repo".into(),
            sha: "abc1234".into(),
            parents: vec!["def5678".into()],
            author: CommitSignature {
                name: "a".into(),
                email: "a@x".into(),
                time: now(),
            },
            committer: CommitSignature {
                name: "a".into(),
                email: "a@x".into(),
                time: now(),
            },
            message: "fix: …".into(),
            branch: Some("main".into()),
            files_changed: vec!["a.rs".into()],
        }
    }

    fn sample_hunk_record() -> HunkRecord {
        HunkRecord {
            diff_hunk_id: "hunk_1".into(),
            session_id: FILESYSTEM_SESSION_ID.into(),
            file_path: "a.rs".into(),
            change_type: "modified".into(),
            line_range_after: Some((42, 57)),
            introduced_by_commit_sha: "abc1234".into(),
            patch_preview: "@@ -40,3 +42,15 @@\n…".into(),
            lines_added: 13,
            lines_removed: 3,
        }
    }

    #[test]
    fn file_payload_has_file_key_and_spec_fields() {
        let p = file_record_to_payload(&sample_file_record());
        let f = p.get("file").unwrap();
        assert_eq!(f.get("path").unwrap(), "/tmp/a.rs");
        assert_eq!(f.get("change_type").unwrap(), "modified");
        assert_eq!(f.get("size_bytes").unwrap(), 42);
        assert!(f.get("observed_at").is_some());
    }

    #[test]
    fn file_payload_includes_old_path_only_when_renamed() {
        let mut r = sample_file_record();
        r.change_type = FileChange::Renamed;
        r.old_path = Some("/tmp/old.rs".into());
        let p = file_record_to_payload(&r);
        assert_eq!(p["file"]["old_path"], "/tmp/old.rs");
        let p2 = file_record_to_payload(&sample_file_record());
        assert!(p2["file"].get("old_path").is_none());
    }

    #[test]
    fn commit_payload_has_git_key_and_spec_fields() {
        let p = commit_record_to_payload(&sample_commit_record());
        let g = &p["git"];
        assert_eq!(g["sha"], "abc1234");
        assert_eq!(g["parents"][0], "def5678");
        assert_eq!(g["author"]["name"], "a");
        assert_eq!(g["branch"], "main");
        assert_eq!(g["files_changed"][0], "a.rs");
    }

    #[test]
    fn hunk_payload_has_hunk_key_and_spec_fields() {
        let p = hunk_record_to_payload(&sample_hunk_record());
        let h = &p["hunk"];
        assert_eq!(h["diff_hunk_id"], "hunk_1");
        assert_eq!(h["file_path"], "a.rs");
        assert_eq!(h["line_range_after"]["start"], 42);
        assert_eq!(h["line_range_after"]["end"], 57);
        assert_eq!(h["lines_added"], 13);
    }

    #[test]
    fn hunk_payload_null_line_range_for_binary() {
        let mut r = sample_hunk_record();
        r.line_range_after = None;
        let p = hunk_record_to_payload(&r);
        assert!(p["hunk"]["line_range_after"].is_null());
    }

    #[test]
    fn canonical_json_is_byte_stable_under_key_reorder() {
        let a = json!({"b": 1, "a": 2, "c": {"y": 3, "x": 4}});
        let b = json!({"a": 2, "b": 1, "c": {"x": 4, "y": 3}});
        assert_eq!(canonical_json(&a), canonical_json(&b));
    }

    #[test]
    fn truncate_patch_preview_returns_shorter_strings_unchanged() {
        assert_eq!(truncate_patch_preview("hi"), "hi");
    }

    #[test]
    fn truncate_patch_preview_appends_marker_when_dropped() {
        let big = "a".repeat(PATCH_PREVIEW_MAX_BYTES + 100);
        let out = truncate_patch_preview(&big);
        assert!(out.ends_with("…[truncated]"));
        // The "a"-prefix preserves at most PATCH_PREVIEW_MAX_BYTES bytes.
        assert!(out.starts_with(&"a".repeat(PATCH_PREVIEW_MAX_BYTES)));
    }

    #[test]
    fn file_change_serialises_to_snake_case() {
        assert_eq!(FileChange::Created.as_str(), "created");
        assert_eq!(FileChange::Modified.as_str(), "modified");
        assert_eq!(FileChange::Deleted.as_str(), "deleted");
        assert_eq!(FileChange::Renamed.as_str(), "renamed");
    }

    async fn fresh_pool() -> sqlx::SqlitePool {
        use crate::db::migrate;
        use sqlx::sqlite::SqlitePoolOptions;
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        migrate(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn store_file_event_persists_and_dedupes() {
        let pool = fresh_pool().await;
        let r = sample_file_record();
        let first = store_file_event(&pool, r.clone(), Utc::now()).await.unwrap();
        assert_eq!(first.accepted_events, 1);
        assert_eq!(first.duplicate_events, 0);
        assert_eq!(first.sessions_touched, vec![FILESYSTEM_SESSION_ID.to_string()]);

        let second = store_file_event(&pool, r, Utc::now()).await.unwrap();
        assert_eq!(second.accepted_events, 0);
        assert_eq!(second.duplicate_events, 1);
        // Self-heal: touched even on duplicate.
        assert_eq!(second.sessions_touched, vec![FILESYSTEM_SESSION_ID.to_string()]);

        let rows =
            crate::db::repo_observed::list_session(&pool, FILESYSTEM_SESSION_ID, 100)
                .await
                .unwrap();
        assert_eq!(rows.len(), 1);
        assert!(matches!(rows[0].kind, EventKind::FileEvent));
        assert_eq!(rows[0].subkind.as_deref(), Some("modified"));
        assert!(matches!(rows[0].actor, Actor::System));
        assert_eq!(rows[0].parser_version, "file_git@0.1.0");
    }

    #[tokio::test]
    async fn store_commit_persists_commit_plus_hunks_and_side_table() {
        let pool = fresh_pool().await;
        let c = sample_commit_record();
        let hunks = vec![sample_hunk_record()];
        let r = store_commit(&pool, c, hunks, Utc::now()).await.unwrap();
        assert_eq!(r.accepted_commits, 1);
        assert_eq!(r.accepted_hunks, 1);
        assert_eq!(r.duplicate_commits, 0);
        assert_eq!(r.duplicate_hunks, 0);
        assert_eq!(r.dropped_hunks_over_limit, 0);

        let rows =
            crate::db::repo_observed::list_session(&pool, FILESYSTEM_SESSION_ID, 100)
                .await
                .unwrap();
        assert_eq!(rows.len(), 2);

        let dh = crate::db::repo_diff_hunk::list_session(&pool, FILESYSTEM_SESSION_ID)
            .await
            .unwrap();
        assert_eq!(dh.len(), 1);
        assert_eq!(dh[0].diff_hunk_id, "hunk_1");
        assert_eq!(dh[0].file_path, "a.rs");
        assert_eq!(dh[0].line_start_after, Some(42));
        assert!(dh[0].related_observed_event_id.is_some());
    }

    #[tokio::test]
    async fn store_commit_dedupes_on_replay() {
        let pool = fresh_pool().await;
        let c = sample_commit_record();
        let hunks = vec![sample_hunk_record()];
        store_commit(&pool, c.clone(), hunks.clone(), Utc::now())
            .await
            .unwrap();
        let r = store_commit(&pool, c, hunks, Utc::now()).await.unwrap();
        assert_eq!(r.accepted_commits, 0);
        assert_eq!(r.duplicate_commits, 1);
        assert_eq!(r.accepted_hunks, 0);
        assert_eq!(r.duplicate_hunks, 1);
        // observed_event still only has 2 rows
        let rows =
            crate::db::repo_observed::list_session(&pool, FILESYSTEM_SESSION_ID, 100)
                .await
                .unwrap();
        assert_eq!(rows.len(), 2);
    }

    #[tokio::test]
    async fn store_commit_drops_surplus_hunks_over_cap() {
        let pool = fresh_pool().await;
        let c = sample_commit_record();
        // Build MAX + 5 unique hunks.
        let mut hunks: Vec<HunkRecord> = Vec::with_capacity(MAX_HUNKS_PER_COMMIT + 5);
        for i in 0..(MAX_HUNKS_PER_COMMIT + 5) {
            let mut h = sample_hunk_record();
            h.diff_hunk_id = format!("hunk_{i}");
            h.file_path = format!("f{i}.rs");
            hunks.push(h);
        }
        let r = store_commit(&pool, c, hunks, Utc::now()).await.unwrap();
        assert_eq!(r.accepted_hunks as usize, MAX_HUNKS_PER_COMMIT);
        assert_eq!(r.dropped_hunks_over_limit, 5);
    }

    #[tokio::test]
    async fn store_commit_binary_hunk_persists_null_line_range() {
        let pool = fresh_pool().await;
        let c = sample_commit_record();
        let mut h = sample_hunk_record();
        h.line_range_after = None;
        h.patch_preview = "<binary>".into();
        let r = store_commit(&pool, c, vec![h], Utc::now()).await.unwrap();
        assert_eq!(r.accepted_hunks, 1);
        let dh = crate::db::repo_diff_hunk::list_session(&pool, FILESYSTEM_SESSION_ID)
            .await
            .unwrap();
        assert_eq!(dh.len(), 1);
        assert!(dh[0].line_start_after.is_none());
        assert!(dh[0].line_end_after.is_none());
    }
}
