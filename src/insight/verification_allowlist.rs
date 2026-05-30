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

/// Wrapper prefixes stripped from a segment before Tier-1/Tier-2 matching.
/// Source: design spec §6.2 ("strip wrappers npx, pnpm dlx, bunx, yarn dlx,
/// poetry run, uv run, sudo, time, env VAR=..").
/// Multi-token wrappers (e.g. `pnpm dlx`) are listed as full token sequences.
/// `env VAR=..` is handled specially (it strips a variable number of `K=V`
/// tokens) and is therefore NOT in this list.
const WRAPPER_PREFIXES: &[&[&str]] = &[
    &["pnpm", "dlx"],
    &["yarn", "dlx"],
    &["poetry", "run"],
    &["uv", "run"],
    &["npx"],
    &["bunx"],
    &["sudo"],
    &["time"],
];

/// Leading executables that, even when a segment contains the `test`/`spec`
/// keyword, are NOT verification commands (file/VCS/shell utilities).
/// Source: design spec §6.2 ("tiny non-exec denylist cat/echo/grep/git/rm/
/// mkdir/cp/mv/ls/find").
const KEYWORD_DENYLIST: &[&str] = &[
    "cat", "echo", "grep", "git", "rm", "mkdir", "cp", "mv", "ls", "find",
];

/// Dry-run / collect-only / list flags: the segment compiles or enumerates
/// tests but does NOT run them, so it is not a verification *run*.
///
/// Slice insight-surface-redesign #2 directive #6 (OVERRIDES the plan's
/// decision to keep dry-runs): `cargo test --no-run`, `cargo nextest … --no-run`,
/// `cargo test --list`, `cargo nextest list`, `pytest --collect-only`, and
/// `vitest … --no-run`-style invocations are excluded via a post-match deny
/// (analogous to the `cargo build --doc` deny in `classify`).
fn is_dry_run(segment: &str) -> bool {
    let tokens: Vec<&str> = segment.split_whitespace().collect();
    // flag-style dry-run markers
    if tokens.iter().any(|t| {
        *t == "--no-run" || *t == "--collect-only" || *t == "--list-tests"
    }) {
        return true;
    }
    // `--list` is dry-run for cargo test / nextest (lists test names).
    if tokens.iter().any(|t| *t == "--list") {
        return true;
    }
    // `cargo nextest list` (subcommand form, no leading dash).
    if tokens.len() >= 3 && tokens[0] == "cargo" && tokens[1] == "nextest" && tokens[2] == "list" {
        return true;
    }
    false
}

/// Strip recognised wrapper prefixes from the front of a (already
/// segment-split, trimmed) command. Strips left-to-right and repeats until no
/// wrapper remains, so `sudo npx vitest run` → `vitest run`.
///
/// `env A=1 B=2 cmd` is handled specially: after an `env` token, all leading
/// `KEY=VALUE` tokens are consumed, then stripping continues on the remainder.
pub fn strip_wrappers(segment: &str) -> &str {
    let mut s = segment.trim();
    loop {
        let tokens: Vec<&str> = s.split_whitespace().collect();
        if tokens.is_empty() {
            return s;
        }

        // env VAR=.. — consume `env` + any leading KEY=VALUE tokens.
        if tokens[0] == "env" {
            let mut consumed = 1; // the `env` token
            while consumed < tokens.len()
                && tokens[consumed].contains('=')
                && !tokens[consumed].starts_with('=')
            {
                consumed += 1;
            }
            if consumed < tokens.len() {
                s = remainder_after(s, consumed);
                continue;
            }
            return s;
        }

        // multi/single-token wrapper prefixes
        let mut matched = false;
        for w in WRAPPER_PREFIXES {
            if tokens.len() > w.len() && tokens[..w.len()] == **w {
                s = remainder_after(s, w.len());
                matched = true;
                break;
            }
        }
        if !matched {
            return s;
        }
    }
}

/// Return the substring of `s` after dropping the first `n` whitespace-split
/// tokens, preserving the original spacing of the remainder.
fn remainder_after(s: &str, n: usize) -> &str {
    let mut rest = s.trim_start();
    for _ in 0..n {
        match rest.find(char::is_whitespace) {
            Some(idx) => rest = rest[idx..].trim_start(),
            None => return "",
        }
    }
    rest
}

/// Classify a single (wrapper-strippable) command segment.
///
/// Returns `Some((command_kind, detection_basis))`:
///   - Tier-1: the wrapper-stripped segment matches the allowlist via
///     `classify` → `(kind, "known_tool")`.
///   - Tier-2: the segment contains the `test`/`spec` keyword, its leading
///     executable is NOT on `KEYWORD_DENYLIST`, and Tier-1 missed →
///     `("test_suite_other", "test_keyword")`.
///   - else `None`.
///
/// Dry-run / collect-only / list segments are denied at both tiers (they
/// compile or enumerate tests but do not run them — slice directive #6).
pub fn classify_segment(segment: &str) -> Option<(&'static str, &'static str)> {
    let stripped = strip_wrappers(segment);

    // Post-match deny: dry-run / collect-only / list is not a verification run.
    if is_dry_run(stripped) {
        return None;
    }

    // Tier-1: known tool (reuses the closed allowlist + cargo build --doc deny).
    if let Some(kind) = classify(stripped) {
        return Some((kind, "known_tool"));
    }

    // Tier-2: keyword fallback. Token-level match avoids substrings like
    // "latest" / "fastest" (we check whitespace-delimited tokens, not the
    // raw string).
    let lead = stripped.split_whitespace().next().unwrap_or("");
    if KEYWORD_DENYLIST.contains(&lead) {
        return None;
    }
    let has_keyword = stripped
        .split(|c: char| {
            c.is_whitespace() || c == '/' || c == ':' || c == '_' || c == '-' || c == '.'
        })
        .any(|t| t == "test" || t == "spec" || t == "tests" || t == "specs");
    if has_keyword {
        return Some(("test_suite_other", "test_keyword"));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_known_wrappers_before_match() {
        assert_eq!(strip_wrappers("npx vitest run"), "vitest run");
        assert_eq!(strip_wrappers("pnpm dlx vitest"), "vitest");
        assert_eq!(strip_wrappers("bunx jest"), "jest");
        assert_eq!(strip_wrappers("yarn dlx mocha"), "mocha");
        assert_eq!(strip_wrappers("poetry run pytest"), "pytest");
        assert_eq!(strip_wrappers("uv run pytest -v"), "pytest -v");
        assert_eq!(strip_wrappers("sudo cargo test"), "cargo test");
        assert_eq!(strip_wrappers("time cargo build"), "cargo build");
        // env VAR=.. (one or more assignments) is stripped
        assert_eq!(strip_wrappers("env RUST_LOG=debug cargo test"), "cargo test");
        assert_eq!(strip_wrappers("env A=1 B=2 pytest"), "pytest");
        // nested wrappers strip left-to-right
        assert_eq!(strip_wrappers("sudo npx vitest run"), "vitest run");
        // no wrapper: unchanged
        assert_eq!(strip_wrappers("cargo test"), "cargo test");
    }

    #[test]
    fn classify_segment_tier1_known_tool_after_wrapper_strip() {
        // npx + compound: the design spec's headline failing case.
        assert_eq!(
            classify_segment("npx vitest run"),
            Some(("test_suite_js", "known_tool"))
        );
        assert_eq!(
            classify_segment("cargo test"),
            Some(("test_suite_rust", "known_tool"))
        );
        // pnpm dlx wrapper around a known tool
        assert_eq!(
            classify_segment("pnpm dlx jest --coverage"),
            Some(("test_suite_js", "known_tool"))
        );
    }

    #[test]
    fn classify_segment_tier2_keyword_fallback() {
        // contains `test`/`spec`, not a known tool, not a denylisted exec.
        assert_eq!(
            classify_segment("./run_integration_test.sh"),
            Some(("test_suite_other", "test_keyword"))
        );
        assert_eq!(
            classify_segment("make spec"),
            Some(("test_suite_other", "test_keyword"))
        );
    }

    #[test]
    fn classify_segment_keyword_denylist_blocks_nonexec() {
        // these CONTAIN `test` but the leading exec is on the non-exec denylist
        assert_eq!(classify_segment("cat test_output.txt"), None);
        assert_eq!(classify_segment("grep test src/lib.rs"), None);
        assert_eq!(classify_segment("git commit -m 'add test'"), None);
        assert_eq!(classify_segment("rm test.tmp"), None);
        assert_eq!(classify_segment("ls tests/"), None);
        assert_eq!(classify_segment("echo running tests"), None);
        assert_eq!(classify_segment("mkdir test"), None);
        assert_eq!(classify_segment("cp a test"), None);
        assert_eq!(classify_segment("mv a test"), None);
        assert_eq!(classify_segment("find . -name test"), None);
    }

    #[test]
    fn classify_segment_no_keyword_no_tool_is_none() {
        assert_eq!(classify_segment("cargo run"), None);
        assert_eq!(classify_segment("npm install"), None);
        assert_eq!(classify_segment("git status"), None);
    }

    #[test]
    fn classify_segment_preserves_cargo_build_doc_deny() {
        // Tier-1 still denies cargo build --doc (delegates to classify()).
        assert_eq!(classify_segment("cargo build --doc"), None);
    }

    #[test]
    fn classify_segment_excludes_dry_run_compile_or_list() {
        // Slice directive #6 override: dry-run / collect-only / list commands
        // compile or enumerate tests but do NOT run them, so they must NOT be
        // detected as verification runs (post-match deny, like cargo build --doc).
        assert_eq!(classify_segment("cargo test --no-run"), None);
        assert_eq!(classify_segment("cargo nextest run --no-run"), None);
        assert_eq!(classify_segment("npx vitest run --no-run"), None);
        assert_eq!(classify_segment("pytest --collect-only"), None);
        assert_eq!(classify_segment("cargo test --list"), None);
        assert_eq!(classify_segment("cargo nextest list"), None);
        // a real run with no dry-run flag still matches
        assert_eq!(
            classify_segment("cargo test"),
            Some(("test_suite_rust", "known_tool"))
        );
    }
}
