//! Allowlist of Bash command patterns that indicate a verification run.
//!
//! This list is **closed** per DEV-S11-03: adding a pattern requires a new
//! slice that provides a real-fixture invariant test for the new pattern.
//! User-configurable allowlists are not supported in MVP.
//!
//! Matching is full-string (`^...$`). Commands with shell metacharacters
//! (`|`, `;`, `&&`) are handled by the extractor, which extracts the leading
//! command before the first such token.
//!
//! Pattern count: 16 (locked by `tests/verification_bash_allowlist.rs`).
//! parser_version: "verification_run@v1"

use once_cell::sync::Lazy;
use regex::Regex;

/// (compiled_regex, command_kind) pairs, built once at first use.
static COMPILED: Lazy<Vec<(Regex, &'static str)>> = Lazy::new(|| {
    PATTERNS
        .iter()
        .map(|(re, kind)| {
            (
                Regex::new(re).unwrap_or_else(|e| panic!("invalid regex {:?}: {}", re, e)),
                *kind,
            )
        })
        .collect()
});

/// Frozen allowlist: exactly 16 (regex, command_kind) pairs.
///
/// Real-data anchoring: patterns 5 (`cargo (?:test|nextest)`) and 7
/// (`cargo build`) were verified against
/// `tests/fixtures/transcripts/real/verification_v01.jsonl` which contains
/// real pairs from session aac68973.
/// Shell metacharacter class that terminates a simple command.
/// Patterns use `[^|;&]+` (one-or-more non-metacharacter chars) to reject
/// composite commands like `cargo test && rm -rf /`.
///
/// All patterns:
/// - Use `^…$` anchoring (full-string match).
/// - Disallow `|`, `;`, `&` via `[^|;&]+` in argument position.
pub const PATTERNS: &[(&str, &str)] = &[
    // 1. npm / pnpm / yarn — `test` and `run test` forms
    (r"^(?:npm|pnpm|yarn)(?: run)? test(?:[: ][^|;&]+)?$",   "test_suite_js"),
    // 2. vitest — with or without `run` subcommand
    (r"^vitest(?: run)?(?:[: ][^|;&]+)?$",                    "test_suite_js"),
    // 3. jest
    (r"^jest(?:[: ][^|;&]+)?$",                               "test_suite_js"),
    // 4. mocha
    (r"^mocha(?:[: ][^|;&]+)?$",                              "test_suite_js"),
    // 5. cargo test and cargo nextest
    (r"^cargo (?:test|nextest)(?:[: ][^|;&]+)?$",             "test_suite_rust"),
    // 6. cargo check
    (r"^cargo check(?:[: ][^|;&]+)?$",                        "build_check"),
    // 7. cargo build — `cargo build --doc` is excluded via the classify()
    //    post-match guard (regex crate does not support lookahead assertions)
    (r"^cargo build(?:[: ][^|;&]+)?$",                        "build"),
    // 8. cargo clippy
    (r"^cargo clippy(?:[: ][^|;&]+)?$",                       "lint"),
    // 9. cargo fmt (with or without --check flag)
    (r"^cargo fmt(?: --check)?(?:[: ][^|;&]+)?$",             "format_check"),
    // 10. pytest
    (r"^pytest(?:[: ][^|;&]+)?$",                             "test_suite_py"),
    // 11. python -m pytest
    (r"^python -m pytest(?:[: ][^|;&]+)?$",                   "test_suite_py"),
    // 12. go test
    (r"^go test(?:[: ][^|;&]+)?$",                            "test_suite_go"),
    // 13. mvn test
    (r"^mvn test(?:[: ][^|;&]+)?$",                           "test_suite_java"),
    // 14. gradle test
    (r"^gradle test(?:[: ][^|;&]+)?$",                        "test_suite_java"),
    // 15. cargo build --release (real-data anchor: appears in verification_v01.jsonl
    //     from session aac68973). Also matched by pattern 7; listed here for
    //     explicit documentation of the real-data fixture command.
    (r"^cargo build --release(?:[: ][^|;&]+)?$",              "build"),
    // 16. cargo fmt without --check (real-data anchor: `cargo fmt` form)
    (r"^cargo fmt$",                                          "format_check"),
];

/// Returns the full allowlist as `(regex_pattern, command_kind)` pairs.
pub fn allowlist_patterns() -> &'static [(&'static str, &'static str)] {
    PATTERNS
}

/// Returns the `command_kind` for the first matching pattern, or `None` if
/// no pattern matches.
///
/// Special case: `cargo build --doc` is explicitly excluded even though
/// `cargo build` matches pattern 7. This is implemented as a post-match
/// guard rather than a regex lookahead because the `regex` crate does not
/// support look-around assertions.
pub fn classify(cmd: &str) -> Option<&'static str> {
    // Explicit deny: `cargo build --doc` is documentation generation, not
    // a verification/build step for regression testing.
    if cmd == "cargo build --doc" || cmd.starts_with("cargo build --doc ") {
        return None;
    }
    for (re, kind) in COMPILED.iter() {
        if re.is_match(cmd) {
            return Some(kind);
        }
    }
    None
}
