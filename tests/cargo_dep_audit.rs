//! Slice-10a — locks that `git2` and `notify` direct dependencies are gone
//! from `Cargo.toml`. Reads the manifest at the workspace root and parses the
//! `[dependencies]` / `[dev-dependencies]` tables as plain text.
//!
//! Transitive dependencies via other crates are tolerated (we cannot control
//! what `sqlx` etc. pull in), but our own direct deps must not list either
//! crate.

use std::path::PathBuf;

fn manifest_path() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir).join("Cargo.toml")
}

fn manifest_text() -> String {
    std::fs::read_to_string(manifest_path()).expect("read Cargo.toml")
}

/// Returns `true` if `line` declares `name` as a TOML key on its own (e.g.
/// `name = ...`, `name.workspace = ...`, or padded variants of either).
/// Section headers like `[dependencies]` and inline-table values are excluded.
fn declares_key(line: &str, name: &str) -> bool {
    let s = line.trim_start();
    if !s.starts_with(name) {
        return false;
    }
    let rest = &s[name.len()..];
    // Next char must be the key separator: whitespace before `=` or a `.` for
    // dotted-table form (`name.workspace = true`).
    let mut chars = rest.chars();
    match chars.next() {
        Some('.') => true,
        Some('=') => true,
        Some(c) if c.is_whitespace() => rest.trim_start().starts_with('='),
        _ => false,
    }
}

#[test]
fn cargo_toml_has_no_git2_direct_dep() {
    let text = manifest_text();
    for line in text.lines() {
        if declares_key(line, "git2") {
            panic!("Cargo.toml still declares git2 as a direct dep: `{line}`");
        }
    }
}

// Slice-10a — `notify` is retained: `src/transcript_tail.rs` (slice-7)
// still depends on it for live JSONL tailing. The removed `notify` consumer
// was `src/watcher.rs` (filesystem watcher), which is gone in slice-10a.
// Test name documents this so future contributors don't try to "clean up"
// notify here.
#[test]
fn notify_stays_because_transcript_tail_needs_it() {
    let text = manifest_text();
    let mut present = false;
    for line in text.lines() {
        if declares_key(line, "notify") {
            present = true;
            break;
        }
    }
    assert!(
        present,
        "notify must remain in Cargo.toml — src/transcript_tail.rs depends on it"
    );
}
