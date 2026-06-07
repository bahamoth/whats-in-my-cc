//! Slice-7 — transcript live tail integration tests.

use sqlx::sqlite::SqlitePoolOptions;
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use wimcc::db::migrate;

const LINE_USER: &str = r#"{"type":"user","uuid":"u1","parentUuid":null,"sessionId":"sess-tail-A","timestamp":"2026-05-21T03:00:00Z","cwd":"/tmp","gitBranch":"main","entrypoint":"cli","userType":"external","version":"2.1.144","isSidechain":false,"promptId":"p1","message":{"role":"user","content":"hello"}}"#;

const LINE_ASSISTANT: &str = r#"{"type":"assistant","uuid":"a1","parentUuid":"u1","sessionId":"sess-tail-A","timestamp":"2026-05-21T03:00:01Z","cwd":"/tmp","gitBranch":"main","entrypoint":"cli","userType":"external","version":"2.1.144","isSidechain":false,"requestId":"req_1","message":{"id":"msg_1","model":"claude-opus-4-7","type":"message","role":"assistant","stop_reason":"end_turn","content":[{"type":"text","text":"hi"}]}}"#;

async fn make_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    pool
}

async fn observed_count(pool: &sqlx::SqlitePool, session_id: &str) -> i64 {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM observed_event WHERE session_id = ?")
        .bind(session_id)
        .fetch_one(pool)
        .await
        .unwrap();
    row.0
}

async fn raw_count(pool: &sqlx::SqlitePool) -> i64 {
    let row: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM raw_event WHERE source_type = 'claude_transcript'")
            .fetch_one(pool)
            .await
            .unwrap();
    row.0
}

fn write_line(path: &std::path::Path, line: &str) {
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .unwrap();
    writeln!(f, "{line}").unwrap();
    f.sync_all().unwrap();
}

async fn await_until<F, Fut>(mut probe: F, timeout: Duration) -> bool
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        if probe().await {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

fn spawn_tail(
    pool: sqlx::SqlitePool,
    root: PathBuf,
) -> (CancellationToken, tokio::task::JoinHandle<()>) {
    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();
    let (live_tx, _) = tokio::sync::broadcast::channel::<wimcc::live::LiveEvent>(64);
    let live_tx = std::sync::Arc::new(live_tx);
    let h = tokio::spawn(async move {
        let _ = wimcc::transcript_tail::run(pool, root, live_tx, cancel_clone).await;
    });
    (cancel, h)
}

#[tokio::test]
async fn tail_ingests_a_new_line_within_one_second() {
    let pool = make_pool().await;
    let tmp = tempfile::tempdir().unwrap();
    let session_file = tmp.path().join("project").join("sess-tail-A.jsonl");
    std::fs::create_dir_all(session_file.parent().unwrap()).unwrap();

    let (cancel, h) = spawn_tail(pool.clone(), tmp.path().to_path_buf());
    // Give the watcher a moment to register.
    tokio::time::sleep(Duration::from_millis(100)).await;

    write_line(&session_file, LINE_USER);
    let pool_probe = pool.clone();
    let ok = await_until(
        || {
            let pool = pool_probe.clone();
            async move { observed_count(&pool, "sess-tail-A").await >= 1 }
        },
        Duration::from_secs(2),
    )
    .await;
    cancel.cancel();
    let _ = h.await;
    assert!(ok, "tail should ingest the new line within 2s");
}

#[tokio::test]
async fn appending_a_second_line_yields_exactly_one_new_row() {
    let pool = make_pool().await;
    let tmp = tempfile::tempdir().unwrap();
    let session_file = tmp.path().join("p").join("sess-tail-B.jsonl");
    std::fs::create_dir_all(session_file.parent().unwrap()).unwrap();

    let (cancel, h) = spawn_tail(pool.clone(), tmp.path().to_path_buf());
    tokio::time::sleep(Duration::from_millis(100)).await;

    write_line(&session_file, LINE_USER);
    let pool_probe = pool.clone();
    await_until(
        || {
            let pool = pool_probe.clone();
            async move { raw_count(&pool).await >= 1 }
        },
        Duration::from_secs(2),
    )
    .await;
    let after_first = raw_count(&pool).await;

    write_line(&session_file, LINE_ASSISTANT);
    let pool_probe = pool.clone();
    await_until(
        || {
            let pool = pool_probe.clone();
            async move { raw_count(&pool).await > after_first }
        },
        Duration::from_secs(2),
    )
    .await;
    let after_second = raw_count(&pool).await;

    cancel.cancel();
    let _ = h.await;
    assert_eq!(
        after_second - after_first,
        1,
        "exactly one new raw row from the second line"
    );
}

#[tokio::test]
async fn restart_does_not_re_ingest_existing_lines() {
    let pool = make_pool().await;
    let tmp = tempfile::tempdir().unwrap();
    let session_file = tmp.path().join("p").join("sess-tail-C.jsonl");
    std::fs::create_dir_all(session_file.parent().unwrap()).unwrap();

    write_line(&session_file, LINE_USER);
    write_line(&session_file, LINE_ASSISTANT);

    // First tail: catch up + watch.
    let (cancel, h) = spawn_tail(pool.clone(), tmp.path().to_path_buf());
    let pool_probe = pool.clone();
    await_until(
        || {
            let pool = pool_probe.clone();
            async move { raw_count(&pool).await >= 2 }
        },
        Duration::from_secs(2),
    )
    .await;
    let initial = raw_count(&pool).await;
    cancel.cancel();
    let _ = h.await;

    // Restart on the same DB + root with no new lines added.
    let (cancel2, h2) = spawn_tail(pool.clone(), tmp.path().to_path_buf());
    tokio::time::sleep(Duration::from_millis(200)).await; // let scan_initial run
    let after_restart = raw_count(&pool).await;
    cancel2.cancel();
    let _ = h2.await;

    assert_eq!(initial, after_restart, "restart must not duplicate rows");
}

#[tokio::test]
async fn missing_root_does_not_error() {
    let pool = make_pool().await;
    let tmp = tempfile::tempdir().unwrap();
    let nonexistent = tmp.path().join("does-not-exist");
    let cancel = CancellationToken::new();
    let (live_tx, _) = tokio::sync::broadcast::channel::<wimcc::live::LiveEvent>(64);
    let live_tx = std::sync::Arc::new(live_tx);
    let res =
        wimcc::transcript_tail::run(pool.clone(), nonexistent, live_tx, cancel).await;
    assert!(res.is_ok());
}
