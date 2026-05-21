use std::time::Duration;

#[test]
fn serve_accepts_watch_and_poll_flags() {
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
            "--git-poll-secs",
            "1",
            "--shutdown-after-ms",
            "300",
            "--auto-migrate",
            "--no-watch-transcripts",
        ])
        .timeout(Duration::from_secs(8))
        .assert()
        .success();
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
