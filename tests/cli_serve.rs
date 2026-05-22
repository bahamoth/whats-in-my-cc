use std::time::Duration;

/// Slice-10a — `--watch` + `--git-poll-secs` flags were removed when the
/// notify watcher + git poller modules were deleted. We assert the binary
/// rejects the flags now so a regression that silently re-adds them is
/// caught at the CLI surface.
#[test]
fn serve_rejects_removed_watch_and_poll_flags() {
    let db = tempfile::NamedTempFile::new().unwrap().into_temp_path();
    let watch_dir = tempfile::tempdir().unwrap();
    let port: u16 = portpicker::pick_unused_port().expect("port");
    assert_cmd::Command::cargo_bin("witmcc")
        .unwrap()
        .args([
            "--db-path",
            db.to_str().unwrap(),
            "serve",
            "--bind",
            "127.0.0.1",
            "--port",
            &port.to_string(),
            "--watch",
            watch_dir.path().to_str().unwrap(),
        ])
        .timeout(Duration::from_secs(5))
        .assert()
        .failure();

    assert_cmd::Command::cargo_bin("witmcc")
        .unwrap()
        .args([
            "--db-path",
            db.to_str().unwrap(),
            "serve",
            "--git-poll-secs",
            "1",
        ])
        .timeout(Duration::from_secs(5))
        .assert()
        .failure();
}

#[tokio::test]
async fn serve_returns_health_ok() {
    // Set up DB
    let db = tempfile::NamedTempFile::new().unwrap().into_temp_path();
    assert_cmd::Command::cargo_bin("witmcc")
        .unwrap()
        .args(["--db-path", db.to_str().unwrap(), "init-db"])
        .assert()
        .success();

    let port: u16 = portpicker::pick_unused_port().expect("port");
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_witmcc"))
        .args([
            "--db-path",
            db.to_str().unwrap(),
            "serve",
            "--bind",
            "127.0.0.1",
            "--port",
            &port.to_string(),
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();

    let url = format!("http://127.0.0.1:{port}/v1/health");
    let mut ok = false;
    for _ in 0..50 {
        if reqwest::get(&url).await.is_ok() {
            ok = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let _ = child.kill();
    let _ = child.wait();
    assert!(ok, "server did not come up at {url}");
}
