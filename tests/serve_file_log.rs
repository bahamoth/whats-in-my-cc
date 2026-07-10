//! Rolling file logger (2026-07-10) — a real `serve` writes a daily-rotating
//! log file next to its DB, and the file is human-readable (Pretty, no ANSI)
//! even when the console `--log-format` is `json`.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn wimcc_bin() -> &'static str {
    env!("CARGO_BIN_EXE_wimcc")
}

#[test]
fn serve_writes_human_readable_rotating_log_next_to_db() {
    let dir = tempfile::tempdir().expect("db tempdir");
    let db = dir.path().join("wimcc.sqlite");
    let config_dir = tempfile::tempdir().expect("config tempdir");
    let port = portpicker::pick_unused_port().expect("free port");

    // --log-format json exercises the decoupling: console would be JSON, but the
    // FILE must stay Pretty + ANSI-free regardless.
    let mut child = Command::new(wimcc_bin())
        .args([
            "--db-path",
            db.to_str().unwrap(),
            "--log-format",
            "json",
            "serve",
            "--port",
            &port.to_string(),
            "--auto-migrate",
            "--no-watch-transcripts",
            "--shutdown-after-ms",
            "1500",
        ])
        .env("WIMCC_CONFIG_DIR", config_dir.path())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn wimcc serve");

    // Wait for self-shutdown (shutdown_after_ms + graceful grace). Poll so a hung
    // server can't wedge the test forever.
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if child.try_wait().expect("try_wait").is_some() {
            break;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("serve did not exit within 15s");
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    // Exactly one wimcc.<date>.log next to the DB (the DB files end in .sqlite*).
    let logs: Vec<std::path::PathBuf> = std::fs::read_dir(dir.path())
        .expect("read_dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            let n = p.file_name().unwrap().to_string_lossy();
            n.starts_with("wimcc") && n.ends_with(".log")
        })
        .collect();
    assert_eq!(
        logs.len(),
        1,
        "expected exactly one wimcc*.log next to the db, got: {logs:?}"
    );

    let body = std::fs::read_to_string(&logs[0]).expect("read log file");
    assert!(!body.is_empty(), "log file should not be empty");
    // Pretty + no ANSI, even though the console format is JSON.
    assert!(
        !body.contains('\u{1b}'),
        "file log must not contain ANSI escape sequences"
    );
    assert!(
        !body.contains("\"level\":"),
        "file log must be Pretty, not JSON, even under --log-format json:\n{body}"
    );
    // The startup marker is emitted before the runtime starts and proves the file
    // layer is wired to the same events as the console.
    assert!(
        body.contains("rotating file log enabled"),
        "startup marker missing from file log:\n{body}"
    );
}
