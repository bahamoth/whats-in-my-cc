//! File/Git observer ingest (slice-5).
//!
//! Source-preserving: each filesystem mutation and each git commit/hunk is
//! persisted as one `RawEvent` (`source_type="file_git"`) + one `ObservedEvent`
//! and surfaces on the synthetic session [`FILESYSTEM_SESSION_ID`].

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{json, Value};

pub const FILESYSTEM_SESSION_ID: &str = "filesystem";

/// Per-commit safety cap so a pathological monorepo commit never blows up the
/// observed_event table. Surplus hunks are dropped and counted in
/// [`CommitIngestResult::dropped_hunks_over_limit`].
pub const MAX_HUNKS_PER_COMMIT: usize = 2000;

/// Hunk `patch_preview` is truncated to this many bytes (UTF-8 safe boundary).
pub const PATCH_PREVIEW_MAX_BYTES: usize = 4 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
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
}
