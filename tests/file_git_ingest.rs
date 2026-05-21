// End-to-end ingest tests for slice-5 file/git observer.
// These exercise the store layer directly (not via the watcher/poller loops,
// whose timing-sensitive tests live in `tests/file_watcher_loop.rs`).

use chrono::Utc;
use sqlx::sqlite::SqlitePoolOptions;
use witmcc::db::{migrate, repo_diff_hunk, repo_graph, repo_observed};
use witmcc::ingest::file_git::{
    extract_commit_records, store_commit, store_file_event, FileChange, FileRecord,
    FILESYSTEM_SESSION_ID,
};
use witmcc::model::observed::EventKind;

async fn fresh_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    pool
}

fn init_repo(tmp: &std::path::Path) -> git2::Repository {
    let repo = git2::Repository::init(tmp).unwrap();
    let mut cfg = repo.config().unwrap();
    cfg.set_str("user.name", "tester").unwrap();
    cfg.set_str("user.email", "t@e").unwrap();
    repo
}

fn commit_file(repo: &git2::Repository, name: &str, body: &[u8], msg: &str) -> git2::Oid {
    use std::io::Write;
    let workdir = repo.workdir().unwrap();
    let path = workdir.join(name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::File::create(&path).unwrap().write_all(body).unwrap();
    let mut idx = repo.index().unwrap();
    idx.add_path(std::path::Path::new(name)).unwrap();
    idx.write().unwrap();
    let oid = idx.write_tree().unwrap();
    let tree = repo.find_tree(oid).unwrap();
    let sig = repo.signature().unwrap();
    let parents: Vec<git2::Commit> = repo
        .head()
        .ok()
        .and_then(|h| h.peel_to_commit().ok())
        .into_iter()
        .collect();
    let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
    repo.commit(Some("HEAD"), &sig, &sig, msg, &tree, &parent_refs)
        .unwrap()
}

#[tokio::test]
async fn commit_emits_git_commit_plus_hunks() {
    let dir = tempfile::tempdir().unwrap();
    let repo = init_repo(dir.path());
    let oid = commit_file(&repo, "a.rs", b"fn main(){}\n", "initial");
    let commit = repo.find_commit(oid).unwrap();
    let (commit_record, hunks) = extract_commit_records(&repo, &commit).unwrap();
    assert!(!hunks.is_empty(), "expected at least one hunk");
    assert!(commit_record.files_changed.iter().any(|f| f == "a.rs"));

    let pool = fresh_pool().await;
    let r = store_commit(&pool, commit_record, hunks, Utc::now(), &witmcc::live::NoopSink)
        .await
        .unwrap();
    assert_eq!(r.accepted_commits, 1);
    assert!(r.accepted_hunks >= 1);

    let rows = repo_observed::list_session(&pool, FILESYSTEM_SESSION_ID, 100)
        .await
        .unwrap();
    let kinds: Vec<&str> = rows.iter().map(|e| e.kind.as_str()).collect();
    assert!(kinds.contains(&"git_commit"));
    assert!(kinds.contains(&"diff_hunk"));
}

#[tokio::test]
async fn hunk_table_row_per_observed_event() {
    let dir = tempfile::tempdir().unwrap();
    let repo = init_repo(dir.path());
    let oid = commit_file(&repo, "a.rs", b"line1\nline2\n", "init");
    let (cr, hr) = extract_commit_records(&repo, &repo.find_commit(oid).unwrap()).unwrap();
    let n_hunks = hr.len();
    let pool = fresh_pool().await;
    store_commit(&pool, cr, hr, Utc::now(), &witmcc::live::NoopSink).await.unwrap();

    let dh = repo_diff_hunk::list_session(&pool, FILESYSTEM_SESSION_ID)
        .await
        .unwrap();
    assert_eq!(dh.len(), n_hunks);
    let count = repo_diff_hunk::count_by_session(&pool, FILESYSTEM_SESSION_ID)
        .await
        .unwrap();
    assert_eq!(count as usize, n_hunks);
}

#[tokio::test]
async fn re_ingest_same_commit_is_no_op() {
    let dir = tempfile::tempdir().unwrap();
    let repo = init_repo(dir.path());
    let oid = commit_file(&repo, "a.rs", b"line1\n", "init");
    let (cr, hr) = extract_commit_records(&repo, &repo.find_commit(oid).unwrap()).unwrap();
    let n_hunks = hr.len();
    let pool = fresh_pool().await;
    store_commit(&pool, cr.clone(), hr.clone(), Utc::now(), &witmcc::live::NoopSink)
        .await
        .unwrap();
    let r2 = store_commit(&pool, cr, hr, Utc::now(), &witmcc::live::NoopSink).await.unwrap();
    assert_eq!(r2.accepted_commits, 0);
    assert_eq!(r2.duplicate_commits, 1);
    assert_eq!(r2.accepted_hunks, 0);
    assert_eq!(r2.duplicate_hunks as usize, n_hunks);
}

#[tokio::test]
async fn binary_file_diff_yields_null_line_range() {
    let dir = tempfile::tempdir().unwrap();
    let repo = init_repo(dir.path());
    // PNG-like header so git treats it as binary.
    let mut blob = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    blob.extend(std::iter::repeat(0u8).take(256));
    let oid = commit_file(&repo, "logo.png", &blob, "add binary");
    let (cr, hr) = extract_commit_records(&repo, &repo.find_commit(oid).unwrap()).unwrap();
    assert!(!hr.is_empty());
    assert!(
        hr.iter().any(|h| h.line_range_after.is_none()),
        "expected at least one binary hunk with null line_range_after"
    );

    let pool = fresh_pool().await;
    store_commit(&pool, cr, hr, Utc::now(), &witmcc::live::NoopSink).await.unwrap();
    let dh = repo_diff_hunk::list_session(&pool, FILESYSTEM_SESSION_ID)
        .await
        .unwrap();
    assert!(dh.iter().any(|r| r.line_start_after.is_none()));
}

#[tokio::test]
async fn graph_for_filesystem_session_has_file_git_nodes() {
    let dir = tempfile::tempdir().unwrap();
    let repo = init_repo(dir.path());
    let oid = commit_file(&repo, "a.rs", b"x\n", "x");
    let (cr, hr) = extract_commit_records(&repo, &repo.find_commit(oid).unwrap()).unwrap();
    let pool = fresh_pool().await;
    store_commit(&pool, cr, hr, Utc::now(), &witmcc::live::NoopSink).await.unwrap();

    let file_record = FileRecord {
        session_id: FILESYSTEM_SESSION_ID.into(),
        path: dir.path().join("a.rs").to_string_lossy().into(),
        change_type: FileChange::Modified,
        old_path: None,
        size_bytes: Some(2),
        observed_at: Utc::now(),
    };
    store_file_event(&pool, file_record, Utc::now(), &witmcc::live::NoopSink).await.unwrap();

    let (nodes, _) = repo_graph::load_session(&pool, FILESYSTEM_SESSION_ID)
        .await
        .unwrap();
    let kinds: std::collections::BTreeSet<&str> =
        nodes.iter().map(|n| n.node_kind.as_str()).collect();
    assert!(kinds.contains("git_commit"), "kinds: {kinds:?}");
    assert!(kinds.contains("diff_hunk"), "kinds: {kinds:?}");
    assert!(kinds.contains("file_event"), "kinds: {kinds:?}");
}

#[tokio::test]
async fn observed_events_for_filesystem_session_visible_via_repo() {
    // Smoke that ObservedEvents for "filesystem" surface through the same
    // repo_observed::list_sessions path as transcript/OTel/hook sessions.
    let dir = tempfile::tempdir().unwrap();
    let repo = init_repo(dir.path());
    let oid = commit_file(&repo, "a.rs", b"hi\n", "init");
    let (cr, hr) = extract_commit_records(&repo, &repo.find_commit(oid).unwrap()).unwrap();
    let pool = fresh_pool().await;
    store_commit(&pool, cr, hr, Utc::now(), &witmcc::live::NoopSink).await.unwrap();

    let sessions = repo_observed::list_sessions(&pool, 100).await.unwrap();
    assert!(
        sessions.iter().any(|s| s.session_id == FILESYSTEM_SESSION_ID),
        "filesystem session missing from list_sessions"
    );

    // EventKind round-trip — confirm we can read git_commit + diff_hunk back.
    let rows = repo_observed::list_session(&pool, FILESYSTEM_SESSION_ID, 100)
        .await
        .unwrap();
    assert!(rows
        .iter()
        .any(|r| matches!(r.kind, EventKind::GitCommit)));
    assert!(rows
        .iter()
        .any(|r| matches!(r.kind, EventKind::DiffHunk)));
}
