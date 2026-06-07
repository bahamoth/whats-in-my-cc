//! Slice-19 — Red-locking tests: token generation + persistence + permissions.
//!
//! These tests will FAIL until `src/security/token.rs` is implemented.
//! Run with:  cargo test --test auth_token

use std::sync::Mutex;

// We need a mutex to serialize env-var mutation across async tests.
// Each test gets a fresh tempdir and sets WIMCC_CONFIG_DIR to it.
static TEST_MUTEX: Mutex<()> = Mutex::new(());

#[tokio::test]
async fn token_is_generated_on_first_call() {
    let _guard = TEST_MUTEX.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("WIMCC_CONFIG_DIR", dir.path());
    let token = wimcc::security::token::ensure_token().unwrap();
    assert!(token.starts_with("wimcc_"), "token should have wimcc_ prefix, got: {token}");
    assert!(token.len() > 20, "token should be long, got: {token}");
    std::env::remove_var("WIMCC_CONFIG_DIR");
}

#[tokio::test]
async fn token_file_is_created_at_correct_path() {
    let _guard = TEST_MUTEX.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("WIMCC_CONFIG_DIR", dir.path());
    let _token = wimcc::security::token::ensure_token().unwrap();
    let tf = dir.path().join("token");
    assert!(tf.exists(), "token file should exist at {}", tf.display());
    std::env::remove_var("WIMCC_CONFIG_DIR");
}

#[cfg(unix)]
#[tokio::test]
async fn token_file_has_mode_0600() {
    let _guard = TEST_MUTEX.lock().unwrap();
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("WIMCC_CONFIG_DIR", dir.path());
    let _token = wimcc::security::token::ensure_token().unwrap();
    let tf = dir.path().join("token");
    let perm = std::fs::metadata(&tf).unwrap().permissions().mode() & 0o777;
    assert_eq!(perm, 0o600, "token file should be mode 0600, got {perm:o}");
    std::env::remove_var("WIMCC_CONFIG_DIR");
}

#[tokio::test]
async fn token_is_reused_across_calls() {
    let _guard = TEST_MUTEX.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("WIMCC_CONFIG_DIR", dir.path());
    let a = wimcc::security::token::ensure_token().unwrap();
    let b = wimcc::security::token::ensure_token().unwrap();
    assert_eq!(a, b, "ensure_token should return the same token on repeated calls");
    std::env::remove_var("WIMCC_CONFIG_DIR");
}

#[cfg(unix)]
#[tokio::test]
async fn refuses_to_load_when_file_overpermissive() {
    let _guard = TEST_MUTEX.lock().unwrap();
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let tf = dir.path().join("token");
    std::fs::write(&tf, "wimcc_test_token").unwrap();
    std::fs::set_permissions(&tf, std::fs::Permissions::from_mode(0o644)).unwrap();
    std::env::set_var("WIMCC_CONFIG_DIR", dir.path());
    let r = wimcc::security::token::load_token_or_err();
    assert!(r.is_err(), "should refuse to load overpermissive token file");
    std::env::remove_var("WIMCC_CONFIG_DIR");
}

#[tokio::test]
async fn rotate_token_generates_a_new_token() {
    let _guard = TEST_MUTEX.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("WIMCC_CONFIG_DIR", dir.path());
    let t1 = wimcc::security::token::ensure_token().unwrap();
    let t2 = wimcc::security::token::rotate_token().unwrap();
    assert_ne!(t1, t2, "rotate_token should generate a new token");
    assert!(t2.starts_with("wimcc_"), "rotated token should have wimcc_ prefix");
    std::env::remove_var("WIMCC_CONFIG_DIR");
}

#[tokio::test]
async fn rotate_token_persists_new_token() {
    let _guard = TEST_MUTEX.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("WIMCC_CONFIG_DIR", dir.path());
    let _ = wimcc::security::token::ensure_token().unwrap();
    let t2 = wimcc::security::token::rotate_token().unwrap();
    // After rotation, ensure_token should return the new token
    let t3 = wimcc::security::token::ensure_token().unwrap();
    assert_eq!(t2, t3, "ensure_token after rotate should return the rotated token");
    std::env::remove_var("WIMCC_CONFIG_DIR");
}
