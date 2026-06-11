//! Slice-11 — allowlist invariant tests (TDD red, Phase 1 commit 1).
//!
//! These tests lock the shape and content of the Bash command allowlist used
//! by the VerificationRun extractor. Adding or removing a pattern requires
//! updating the count here AND providing a curated sample + deny sample.
//!
//! Real-data anchoring: the sample commands in `classify_matches_curated_commands`
//! include exact prefixes drawn from the `verification_v01.jsonl` real fixture
//! (e.g., `cargo test --test api`, `cargo build`).

use wimcc::insight::verification_allowlist::{allowlist_patterns, classify};

#[test]
fn allowlist_has_expected_pattern_count() {
    // Locked count — must be updated when patterns are added/removed.
    assert_eq!(
        allowlist_patterns().len(),
        16,
        "pattern count changed; if intentional, update this count AND provide samples"
    );
}

#[test]
fn every_pattern_compiles_as_regex() {
    for (re, _kind) in allowlist_patterns() {
        regex::Regex::new(re).unwrap_or_else(|_| panic!("invalid regex: {re}"));
    }
}

#[test]
fn classify_matches_curated_commands() {
    let samples: &[(&str, &str)] = &[
        ("npm test", "test_suite_js"),
        ("npm run test", "test_suite_js"),
        ("npm run test:unit", "test_suite_js"),
        ("pnpm test", "test_suite_js"),
        ("yarn test", "test_suite_js"),
        ("vitest run", "test_suite_js"),
        ("vitest", "test_suite_js"),
        ("jest", "test_suite_js"),
        ("jest --coverage", "test_suite_js"),
        ("mocha", "test_suite_js"),
        ("cargo test", "test_suite_rust"),
        ("cargo test --test api", "test_suite_rust"),
        ("cargo test --lib ingest::hook", "test_suite_rust"),
        ("cargo nextest run", "test_suite_rust"),
        ("cargo nextest run --all", "test_suite_rust"),
        ("cargo check", "build_check"),
        ("cargo check --workspace", "build_check"),
        ("cargo build", "build"),
        ("cargo build --release", "build"),
        ("cargo clippy", "lint"),
        ("cargo clippy -- -D warnings", "lint"),
        ("cargo fmt --check", "format_check"),
        ("cargo fmt", "format_check"),
        ("pytest", "test_suite_py"),
        ("pytest -v tests/", "test_suite_py"),
        ("python -m pytest", "test_suite_py"),
        ("python -m pytest --tb=short", "test_suite_py"),
        ("go test ./...", "test_suite_go"),
        ("go test -v ./...", "test_suite_go"),
        ("mvn test", "test_suite_java"),
        ("gradle test", "test_suite_java"),
    ];
    for (cmd, want_kind) in samples {
        let got = classify(cmd);
        assert_eq!(
            got,
            Some(*want_kind),
            "command {cmd:?} should classify as {want_kind:?}, got {got:?}"
        );
    }
}

#[test]
fn classify_rejects_non_test_commands() {
    let deny: &[&str] = &[
        "npm install",
        "npm ci",
        "cargo run",
        "cargo doc",
        "cargo clean",
        "git status",
        "ls -la",
        // composite — the full string doesn't match anchor; extractor handles splitting
        "cargo test && rm -rf /",
        // Not anchored at start:
        "echo cargo test",
        // build --doc is not a verification command
        "cargo build --doc",
    ];
    for cmd in deny {
        assert!(
            classify(cmd).is_none(),
            "command {:?} should not classify as verification; got {:?}",
            cmd,
            classify(cmd)
        );
    }
}
