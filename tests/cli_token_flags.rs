//! Slice-19 — Red-locking tests: CLI flags --print-token and --rotate-token.
//!
//! These tests will FAIL until the flags are wired into `src/cli.rs` + `src/main.rs`.

use assert_cmd::Command;

fn wimcc() -> Command {
    Command::cargo_bin("wimcc").unwrap()
}

#[test]
fn print_token_prints_to_stderr_and_exits_zero() {
    let dir = tempfile::tempdir().unwrap();
    let out = wimcc()
        .env("WIMCC_CONFIG_DIR", dir.path())
        .args(["serve", "--print-token"])
        .assert()
        .success();
    let stderr = String::from_utf8_lossy(&out.get_output().stderr);
    assert!(
        stderr.contains("wimcc_"),
        "stderr should contain the token (wimcc_ prefix), got: {stderr}"
    );
}

#[test]
fn rotate_token_changes_token_file() {
    let dir = tempfile::tempdir().unwrap();

    // First, generate initial token via --print-token
    wimcc()
        .env("WIMCC_CONFIG_DIR", dir.path())
        .args(["serve", "--print-token"])
        .assert()
        .success();

    let t1 = std::fs::read_to_string(dir.path().join("token")).unwrap();

    // Rotate
    wimcc()
        .env("WIMCC_CONFIG_DIR", dir.path())
        .args(["serve", "--rotate-token"])
        .assert()
        .success();

    let t2 = std::fs::read_to_string(dir.path().join("token")).unwrap();
    assert_ne!(t1, t2, "--rotate-token should change the token file");
    assert!(
        t2.starts_with("wimcc_"),
        "new token should still have wimcc_ prefix, got: {t2}"
    );
}

#[test]
fn rotate_token_prints_new_token_to_stderr() {
    let dir = tempfile::tempdir().unwrap();
    wimcc()
        .env("WIMCC_CONFIG_DIR", dir.path())
        .args(["serve", "--print-token"])
        .assert()
        .success();

    let out = wimcc()
        .env("WIMCC_CONFIG_DIR", dir.path())
        .args(["serve", "--rotate-token"])
        .assert()
        .success();
    let stderr = String::from_utf8_lossy(&out.get_output().stderr);
    assert!(
        stderr.contains("wimcc_"),
        "rotate should print new token to stderr, got: {stderr}"
    );
}
