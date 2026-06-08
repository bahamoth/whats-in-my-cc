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
                Regex::new(re).unwrap_or_else(|e| panic!("invalid regex {re:?}: {e}")),
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

/// Wrapper prefixes stripped from a segment before classify_segment matching.
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
    if tokens.contains(&"--list") {
        return true;
    }
    // `cargo nextest list` (subcommand form, no leading dash).
    if tokens.len() >= 3 && tokens[0] == "cargo" && tokens[1] == "nextest" && tokens[2] == "list" {
        return true;
    }
    false
}

/// Strip trailing stream-redirect idioms from a (segment-split) command so the
/// Tier-1 regexes — which forbid `&` via `[^|;&]+` — still match. Output
/// redirects do not change *which* tool ran; they only capture its streams.
///
/// Handles the common trailing forms (after the leading command + args):
///   - `2>&1`, `>&2`, `1>&2`           (fd duplication)
///   - `> file`, `2> file`, `&> file`  (truncating redirect to a target)
///   - `>> file`, `2>> file`           (appending redirect)
///
/// A `cmd 2>&1` → `cmd`. A `cmd > out.log` → `cmd`. Only contiguous trailing
/// redirect tokens are removed; a redirect in the middle is left intact (these
/// segments are already pipe-split, so a trailing redirect is the common case).
pub fn strip_redirects(segment: &str) -> &str {
    let mut s = segment.trim_end();
    loop {
        let trimmed = s.trim_end();
        // fd-duplication form: token ending in `>&N` or exactly `2>&1` etc.
        if let Some(last) = trimmed.split_whitespace().next_back() {
            if is_fd_dup_token(last) {
                // drop the trailing token
                let cut = trimmed.len() - last.len();
                s = trimmed[..cut].trim_end();
                continue;
            }
        }
        // `OP target` form: a redirect operator token followed by a target.
        let tokens: Vec<&str> = trimmed.split_whitespace().collect();
        if tokens.len() >= 2 && is_redirect_op(tokens[tokens.len() - 2]) {
            // drop the last two tokens (operator + target)
            let op = tokens[tokens.len() - 2];
            if let Some(pos) = trimmed.rfind(op) {
                s = trimmed[..pos].trim_end();
                continue;
            }
        }
        return trimmed;
    }
}

/// True for fd-duplication redirect tokens like `2>&1`, `>&2`, `1>&2`, `&>file`.
fn is_fd_dup_token(tok: &str) -> bool {
    // `2>&1`, `>&2`, `1>&2`
    if tok.contains(">&") {
        return true;
    }
    false
}

/// True for standalone redirect operator tokens (`>`, `>>`, `2>`, `2>>`, `&>`).
fn is_redirect_op(tok: &str) -> bool {
    matches!(tok, ">" | ">>" | "2>" | "2>>" | "&>" | "1>" | "1>>")
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
///   - else `None`.
///
/// Tier-2 keyword fallback ("`test`/`spec` 토큰이 있으면 추정") は **제거됨**.
/// multi-line Bash(commit 메시지·heredoc)에서 split된 산문이 phantom verification
/// run을 만들었다. known_tool(결정론 allowlist)만 인정한다.
/// .wimcc-analysis.sqlite 기준 test_keyword는 13/195건만 담고
/// false-positive 클래스 전체를 만들었다. spec F2.
///
/// Dry-run / collect-only / list segments are denied (they compile or
/// enumerate tests but do not run them — slice directive #6).
pub fn classify_segment(segment: &str) -> Option<(&'static str, &'static str)> {
    let stripped = strip_wrappers(strip_redirects(segment));

    // Dry-run / collect-only / list 세그먼트는 verification run이 아니다.
    if is_dry_run(stripped) {
        return None;
    }

    // Tier-1만: known tool(결정론 allowlist). 과거 Tier-2 keyword fallback
    // ("세그먼트에 test/spec 토큰이 있으면 테스트일 것")은 제거됨 — multi-line
    // Bash(commit 메시지·heredoc)에서 split된 산문이 phantom run을 만들었고,
    // 이는 휴리스틱 추정이다. .wimcc-analysis.sqlite 기준 test_keyword는
    // measured 13/195건만 담고 false-positive 클래스 전체를 만들었다.
    classify(stripped).map(|kind| (kind, "known_tool"))
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
    fn classify_segment_drops_prose_false_positives() {
        // Real-data anchoring (.wimcc-analysis.sqlite): 아래 산문 줄들은 multi-line
        // Bash(commit -m 본문·heredoc)에서 split돼 제거 대상 Tier-2 keyword fallback에
        // phantom test run으로 잡혔다. Tier-1(known_tool)에는 매칭되지 않으므로 None이어야.
        assert_eq!(
            classify_segment("- CI 회복: scripts/run-tests.mjs 신설 (cross-platform glob)"),
            None
        );
        assert_eq!(
            classify_segment("- SA1 Metica activation was previously gated on completion of Airflux test"),
            None
        );
        assert_eq!(
            classify_segment("declare the contract at spec §1.9. Pages live as `<slug>.md`"),
            None
        );
    }

    #[test]
    fn classify_segment_known_tool_still_matches() {
        assert_eq!(classify_segment("cargo test"), Some(("test_suite_rust", "known_tool")));
        assert_eq!(classify_segment("npx vitest run"), Some(("test_suite_js", "known_tool")));
    }

    #[test]
    fn classify_segment_non_allowlist_runner_no_longer_matches() {
        // Tier-2 제거 trade-off(정직): 비-allowlist 실 러너는 더 이상 잡지 않는다.
        // 거짓 phantom보다 일부 누락이 낫다. 필요 시 allowlist를 결정론적으로 확장.
        assert_eq!(classify_segment("./run_integration_test.sh"), None);
        assert_eq!(classify_segment("make spec"), None);
    }

    #[test]
    fn classify_segment_path_with_tests_dir_is_not_a_run() {
        // Real-data anchoring: this project's transcripts contain
        //   `./target/debug/wimcc ingest tests/fixtures/transcripts/x.jsonl`
        // — a non-test command whose argument PATH contains a `tests/` dir.
        // It is not on the Tier-1 known-tool allowlist, so it is None. (This
        // was a Tier-2 false-positive class before the keyword fallback was
        // removed — spec F2; now None falls out of the allowlist miss directly.)
        assert_eq!(
            classify_segment("./target/debug/wimcc ingest tests/fixtures/transcripts/minimal_session.jsonl"),
            None
        );
        assert_eq!(
            classify_segment("wimcc ingest tests/fixtures/a.jsonl"),
            None
        );
    }

    #[test]
    fn classify_segment_non_allowlist_commands_with_test_token_are_none() {
        // these CONTAIN a `test` token but are NOT on the Tier-1 known-tool
        // allowlist; with Tier-2 keyword fallback removed (spec F2) they are
        // all None (formerly some were blocked by the now-deleted KEYWORD_DENYLIST).
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
