//! Task 11 — event tag 분류기 core 이전: webui eventTags.test.ts의 분류
//! 케이스 1:1 이식 (패리티 잠금). 분류(측정)는 Rust가 소유하고 webui는
//! 서버 값을 소비한다 — 어휘가 표현 계층에 갇혀 MCP 소비자가 raw tool_name만
//! 보던 비대칭(리뷰 발견 5)의 해소.

use serde_json::json;
use wimcc::insight::event_tags::{
    classify_tool_call, meaningful_command, segment_command, TagDisposition, TagOutcome,
    BASH_FIRST_TOKEN_TAGS, MCP_SERVER_TOOL_TAGS, TOOL_SUBCOMMAND_TAGS,
};

fn bash(command: &str) -> TagOutcome {
    classify_tool_call(Some("Bash"), &json!({"input": {"command": command}}))
}
fn read(file_path: &str) -> TagOutcome {
    classify_tool_call(Some("Read"), &json!({"input": {"file_path": file_path}}))
}
fn edit(file_path: &str) -> TagOutcome {
    classify_tool_call(Some("Edit"), &json!({"input": {"file_path": file_path}}))
}
fn write_t(file_path: &str) -> TagOutcome {
    classify_tool_call(Some("Write"), &json!({"input": {"file_path": file_path}}))
}
fn tag(o: &TagOutcome) -> Option<&'static str> {
    o.value
}

#[test]
fn read_file_search_inspect() {
    assert_eq!(tag(&bash("grep -n foo src")), Some("read.file"));
    assert_eq!(tag(&bash("find . -name \"*.rs\"")), Some("read.file"));
    assert_eq!(tag(&bash("ls -la")), Some("read.file"));
    assert_eq!(tag(&bash("cat Cargo.toml")), Some("read.file"));
    assert_eq!(tag(&bash("sed -n '1,5p' x")), Some("read.file"));
}

#[test]
fn read_proc_db_web_and_date() {
    assert_eq!(tag(&bash("ps -p 1")), Some("read.proc"));
    assert_eq!(tag(&bash("lsof -ti :5175")), Some("read.proc"));
    assert_eq!(tag(&bash("sqlite3 db .tables")), Some("read.db"));
    assert_eq!(tag(&bash("curl -s http://x")), Some("read.web"));
    assert_eq!(
        tag(&bash("date -u +\"%Y-%m-%dT%H:%M:%SZ\"")),
        Some("read.proc")
    );
}

#[test]
fn git_read_vs_write_by_subcommand() {
    assert_eq!(tag(&bash("git status")), Some("read.vcs"));
    assert_eq!(tag(&bash("git diff HEAD")), Some("read.vcs"));
    assert_eq!(tag(&bash("git fetch")), Some("read.vcs"));
    assert_eq!(tag(&bash("git commit -m x")), Some("write.vcs"));
    assert_eq!(tag(&bash("git push")), Some("write.vcs"));
    assert_eq!(tag(&bash("git mv a b")), Some("write.vcs"));
    assert_eq!(tag(&bash("gh pr create")), Some("write.vcs"));
}

#[test]
fn multiplexer_subcommands_decide_verb() {
    assert_eq!(tag(&bash("cargo build --release")), Some("build.code"));
    assert_eq!(tag(&bash("cargo test --all")), Some("test.code"));
    assert_eq!(tag(&bash("cargo run -- serve")), Some("run.code"));
    assert_eq!(tag(&bash("cargo clippy")), Some("lint.code"));
    assert_eq!(tag(&bash("cargo add serde")), Some("write.deps"));
    assert_eq!(tag(&bash("npm test")), Some("test.code"));
    assert_eq!(tag(&bash("npm run dev")), Some("run.code"));
    assert_eq!(tag(&bash("npm install")), Some("write.deps"));
    assert_eq!(tag(&bash("go build ./...")), Some("build.code"));
}

#[test]
fn single_purpose_tools_and_tsc_flip() {
    assert_eq!(tag(&bash("make")), Some("build.code"));
    assert_eq!(tag(&bash("vitest run")), Some("test.code"));
    assert_eq!(tag(&bash("eslint .")), Some("lint.code"));
    assert_eq!(tag(&bash("tsc -p .")), Some("build.code"));
    assert_eq!(tag(&bash("tsc --noEmit")), Some("lint.code"));
}

#[test]
fn run_code_interpreters_and_paths() {
    assert_eq!(tag(&bash("python3 script.py")), Some("run.code"));
    assert_eq!(tag(&bash("node x.js")), Some("run.code"));
    assert_eq!(tag(&bash("bash tests/x.sh")), Some("run.code"));
    assert_eq!(tag(&bash("./target/release/wimcc serve")), Some("run.code"));
    assert_eq!(tag(&bash("/usr/local/bin/foo")), Some("run.code"));
    assert_eq!(
        tag(&bash("tests/structural/x.sh ch-prod")),
        Some("run.code")
    );
    assert_eq!(tag(&bash("target/debug/wimcc --help")), Some("run.code"));
    assert_eq!(
        tag(&bash(".claude/skills/ch/scripts/ch ch-prod")),
        Some("run.code")
    );
}

#[test]
fn write_delete_deps() {
    assert_eq!(tag(&bash("mkdir -p a/b")), Some("write.file"));
    assert_eq!(tag(&bash("cp a b")), Some("write.file"));
    assert_eq!(tag(&bash("chmod +x x")), Some("write.file"));
    assert_eq!(tag(&bash("rm -rf target")), Some("delete.file"));
    assert_eq!(tag(&bash("mv a b")), Some("delete.file"));
    assert_eq!(tag(&bash("pip install cairosvg")), Some("write.deps"));
}

#[test]
fn compounds_classify_by_first_meaningful() {
    assert_eq!(
        tag(&bash("cd /repo && git add -A && git status")),
        Some("write.vcs")
    );
    assert_eq!(tag(&bash("cd x && grep y")), Some("read.file"));
    assert_eq!(tag(&bash("grep y > out.txt")), Some("read.file"));
    assert_eq!(tag(&bash("grep a | grep b | wc -l")), Some("read.file"));
    assert_eq!(tag(&bash("cd x && rm -rf y")), Some("delete.file"));
}

#[test]
fn multiplexer_global_flags_skipped() {
    assert_eq!(tag(&bash("git -C .. diff --stat")), Some("read.vcs"));
    assert_eq!(tag(&bash("git -C /repo status --short")), Some("read.vcs"));
    assert_eq!(
        tag(&bash("git -c user.name=x commit -m y")),
        Some("write.vcs")
    );
    assert_eq!(tag(&bash("git --no-pager log")), Some("read.vcs"));
    assert_eq!(tag(&bash("cargo +1.86.0 build 2>&1")), Some("build.code"));
}

#[test]
fn timeout_wrapper_unwrapped() {
    assert_eq!(tag(&bash("timeout 180 npm run dev")), Some("run.code"));
    assert_eq!(tag(&bash("timeout 60 cargo test")), Some("test.code"));
    assert_eq!(tag(&bash("timeout 5s git status")), Some("read.vcs"));
    assert_eq!(
        tag(&bash("timeout -s SIGTERM 5 cargo test")),
        Some("test.code")
    );
    assert_eq!(tag(&bash("timeout -k 10 30 npm test")), Some("test.code"));
    assert_eq!(
        tag(&bash("timeout --signal=KILL 10 npm test")),
        Some("test.code")
    );
}

#[test]
fn just_and_shasum() {
    assert_eq!(tag(&bash("just webui-build")), Some("run.code"));
    assert_eq!(tag(&bash("shasum -a 256 file.pdf")), Some("read.file"));
}

#[test]
fn control_vs_unmatched() {
    assert_eq!(
        bash("cd /tmp && echo done").disposition,
        TagDisposition::Control
    );
    assert_eq!(bash("cd /tmp").disposition, TagDisposition::Control);
    assert_eq!(bash("frobnicate x").disposition, TagDisposition::Unmatched);
    assert_eq!(
        bash("git frobnicate").disposition,
        TagDisposition::Unmatched
    );
    assert_eq!(bash("npm frob").disposition, TagDisposition::Unmatched);
}

#[test]
fn strips_leading_comment_lines() {
    assert_eq!(tag(&bash("# explore\ngrep -r x src")), Some("read.file"));
}

#[test]
fn joins_backslash_continuations() {
    assert_eq!(
        tag(&bash("cargo build \\\n  --release")),
        Some("build.code")
    );
    assert_eq!(
        tag(&bash("grep -n foo \\\n  src/lib.rs | \\\n  head -5")),
        Some("read.file")
    );
    assert_eq!(segment_command("grep foo \\\n  bar"), vec!["grep foo bar"]);
}

#[test]
fn splits_on_newlines() {
    assert_eq!(tag(&bash("cd /x\ngrep y")), Some("read.file"));
    assert_eq!(tag(&bash("cargo build\ncargo test")), Some("build.code"));
}

#[test]
fn does_not_mis_split_redirects() {
    assert_eq!(tag(&bash("grep x src 2>&1 | head")), Some("read.file"));
    assert_eq!(tag(&bash("cargo test 2>&1")), Some("test.code"));
}

#[test]
fn skips_assignment_prefixes() {
    assert_eq!(tag(&bash("VAULT=/x grep y")), Some("read.file"));
    assert_eq!(tag(&bash("FOO=/x\ncat f")), Some("read.file"));
    assert_eq!(bash("FOO=bar").disposition, TagDisposition::Control);
}

#[test]
fn loop_keywords_are_control() {
    assert_eq!(
        tag(&bash("for f in *; do grep x \"$f\"; done")),
        Some("read.file")
    );
    assert_eq!(tag(&bash("[ -f x ] && cat y")), Some("read.file"));
}

#[test]
fn read_by_extension() {
    assert_eq!(tag(&read("src/a.rs")), Some("read.code"));
    assert_eq!(tag(&read("webui/x.tsx")), Some("read.code"));
    assert_eq!(tag(&read("README.md")), Some("read.docs"));
    assert_eq!(tag(&read("Cargo.toml")), Some("read.config"));
    assert_eq!(tag(&read("data.json")), Some("read.data"));
    assert_eq!(tag(&read("tasks/run.output")), Some("read.data"));
    assert_eq!(tag(&read("scripts/fetch.py")), Some("read.code"));
}

#[test]
fn edit_write_by_extension_and_unmatched() {
    assert_eq!(tag(&edit("src/a.rs")), Some("write.code"));
    assert_eq!(tag(&edit("webui/x.tsx")), Some("write.code"));
    assert_eq!(tag(&edit("README.md")), Some("write.docs"));
    assert_eq!(tag(&edit("Cargo.toml")), Some("write.config"));
    assert_eq!(tag(&write_t("out/data.json")), Some("write.data"));
    assert_eq!(tag(&write_t("tasks/run.output")), Some("write.data"));
    assert_eq!(tag(&edit("scripts/fetch.py")), Some("write.code"));
    assert_eq!(edit("Makefile").disposition, TagDisposition::Unmatched);
}

#[test]
fn non_file_tools_get_no_chip() {
    let o = classify_tool_call(Some("Task"), &json!({}));
    assert_eq!(o.disposition, TagDisposition::Control);
    assert_eq!(o.value, None);
}

#[test]
fn meaningful_command_strips_leading_control() {
    assert_eq!(
        meaningful_command("cd /repo && git add -A && git status"),
        "git add -A && git status"
    );
    assert_eq!(meaningful_command("cd x && grep y"), "grep y");
    assert_eq!(meaningful_command("grep a | grep b"), "grep a | grep b");
    assert_eq!(meaningful_command("rm -f x && ls"), "rm -f x && ls");
    assert_eq!(meaningful_command("cd /tmp"), "cd /tmp");
}

// ── untagged-loop 집계 토큰 의미론 (frontend collectUntagged의 서버측 절반) ──

#[test]
fn token_aggregates_unmatched_by_meaningful_first_token() {
    let o = bash("cd /repo && frobnicate c");
    assert_eq!(o.disposition, TagDisposition::Unmatched);
    assert_eq!(o.token.as_deref(), Some("frobnicate"));
}

#[test]
fn token_for_unknown_multiplexer_sub_is_tool_sub() {
    assert_eq!(
        bash("git frobnicate").token.as_deref(),
        Some("git frobnicate")
    );
    assert_eq!(
        bash("git frobnicate --x").token.as_deref(),
        Some("git frobnicate")
    );
    // 글로벌 플래그를 지나서도 같은 sub로 집계된다.
    assert_eq!(
        bash("git -C .. frobnicate").token.as_deref(),
        Some("git frobnicate")
    );
}

#[test]
fn token_noise_suppression() {
    // 서브셸 괄호는 벗겨져 실제 커맨드로 집계된다.
    let sub = bash("(cd webui && frobnicate run)");
    assert_eq!(sub.disposition, TagDisposition::Unmatched);
    assert_eq!(sub.token.as_deref(), Some("frobnicate"));
    // 루프 제어·플래그 라인은 control — 집계 토큰을 내지 않는다.
    assert_eq!(bash("break").disposition, TagDisposition::Control);
    assert_eq!(bash("continue").disposition, TagDisposition::Control);
    let flagline = bash("cd /x\n  -s http://y");
    assert_eq!(flagline.disposition, TagDisposition::Control);
    assert!(flagline.token.is_none());
}

#[test]
fn token_comment_and_assignment_noise_stripped() {
    assert_eq!(
        bash("# explore\nfrobnicate view").token.as_deref(),
        Some("frobnicate")
    );
    assert_eq!(
        bash("VAULT=/x\nfrobnicate list").token.as_deref(),
        Some("frobnicate")
    );
}

#[test]
fn token_for_read_is_extension_or_basename() {
    // 2026-06-16 tagging loop: .diff/.patch are saved diff/patch output text
    // Claude reads back as input → data object (was Unmatched).
    let diff = read("/tmp/pr41.diff");
    assert_eq!(diff.disposition, TagDisposition::Tagged);
    assert_eq!(tag(&diff), Some("read.data"));
    assert_eq!(diff.token.as_deref(), Some("diff"));
    // A no-extension file NOT in FILENAME_OBJECT stays Unmatched, token=basename.
    // (`.gitignore`/`justfile` now tag via FILENAME_OBJECT — see the 2026-06-23 block.)
    let unknown = read("/repo/Makefile");
    assert_eq!(unknown.disposition, TagDisposition::Unmatched);
    assert_eq!(unknown.token.as_deref(), Some("Makefile"));
}

// 2026-06-16 tagging hygiene loop — rules added from surfaced untagged tokens.
#[test]
fn tagging_loop_2026_06_16_additions() {
    // pnpm vitest — vitest as a pnpm subcommand (first-token vitest already maps).
    assert_eq!(tag(&bash("pnpm vitest run src")), Some("test.code"));
    // printf — a shell builtin emit, like echo → Control (not a meaningful verb).
    assert_eq!(bash("printf 'x %s' y").disposition, TagDisposition::Control);
    // nohup — a transparent wrapper like timeout; tag the inner command.
    assert_eq!(tag(&bash("nohup cargo build")), Some("build.code"));
    assert_eq!(tag(&bash("nohup npm run dev")), Some("run.code"));
    // .vue — Vue single-file component is code.
    assert_eq!(tag(&read("src/App.vue")), Some("read.code"));
    assert_eq!(tag(&write_t("src/App.vue")), Some("write.code"));
    // .diff/.patch — saved diff/patch output text. NOT read-only: Claude both
    // writes them (saving output) and reads them back as input → data both ways.
    assert_eq!(tag(&read("/tmp/fix.patch")), Some("read.data"));
    assert_eq!(tag(&write_t("/tmp/out.diff")), Some("write.data"));
    assert_eq!(tag(&write_t("/tmp/fix.patch")), Some("write.data"));
    // `diff` the COMMAND (distinct from the .diff extension) compares files → read.file.
    assert_eq!(tag(&bash("diff a.txt b.txt")), Some("read.file"));
}

// 2026-06-23 tagging hygiene loop — rules added from surfaced untagged tokens
// (untagged-bash.ts --all). Evidenced counts in parens.
#[test]
fn tagging_loop_2026_06_23_additions() {
    // Image assets (jpg 29, png 26) — new `image` object. jpeg/gif/svg/webp are
    // the same closed category (read/written via Read/Edit), not a speculative
    // generalization. Read → read.image, Write/Edit → write.image.
    assert_eq!(tag(&read("assets/slide_qa-01.jpg")), Some("read.image"));
    assert_eq!(tag(&read("/tmp/nc-logo-white.png")), Some("read.image"));
    assert_eq!(tag(&read("icon.svg")), Some("read.image"));
    assert_eq!(tag(&write_t("out/diagram.png")), Some("write.image"));
    // Shell script extension — `.sh` file content is code (distinct from the
    // `sh` COMMAND, which is run.code via BASH_FIRST_TOKEN_TAGS).
    assert_eq!(tag(&read("hooks/wimcc-forward.sh")), Some("read.code"));
    assert_eq!(tag(&edit("scripts/build.sh")), Some("write.code"));
    // No-extension filenames (.gitignore 31, justfile 7) → FILENAME_OBJECT
    // fallback (ext_of yields "" so EXT_OBJECT can't catch them).
    assert_eq!(tag(&read("/repo/.gitignore")), Some("read.config"));
    assert_eq!(tag(&edit("/repo/.gitignore")), Some("write.config"));
    assert_eq!(tag(&read("/repo/justfile")), Some("read.code"));
    assert_eq!(tag(&edit("/repo/justfile")), Some("write.code"));
    // First-token commands: pdftotext (9) reads/extracts a PDF → read.docs;
    // serena (4) runs the serena CLI tool → run.code.
    assert_eq!(
        tag(&bash("pdftotext -layout in.pdf out.txt")),
        Some("read.docs")
    );
    assert_eq!(
        tag(&bash("serena project index --language rust")),
        Some("run.code")
    );
    // git check-ignore (5) queries ignore status → read.vcs.
    assert_eq!(tag(&bash("git check-ignore -v .claude")), Some("read.vcs"));
}

// 2026-06-29 tagging hygiene loop — universal CLIs/extensions surfaced by
// untagged-bash.ts --all. Multiplexers (aws/claude/uv/rustup) and verb-ambiguous
// subs (cargo metadata/clean/tree/doc, npm list/init) are deferred, not
// force-tagged; `env`/`time` are command wrappers (env VAR=x cmd) so they're not
// single-verb; `qmd` is a company one-off CLI (`qmd search …`, NOT the .qmd
// Quarto extension — sample-verified) so it's deferred; shell-tokenizer
// fragments (vars/heredocs/regex/company paths) are not dictionary-addressable.
#[test]
fn tagging_loop_2026_06_29_additions() {
    // Source/doc/config extensions. `.zsh` script content is code (like `.sh`).
    assert_eq!(tag(&read("Assets/Scripts/Player.cs")), Some("read.code")); // C#
    assert_eq!(tag(&edit("server.mjs")), Some("write.code")); // ES module
    assert_eq!(tag(&read(".zsh.config/bastion.zsh")), Some("read.code"));
    assert_eq!(tag(&read("nginx.conf")), Some("read.config"));
    assert_eq!(tag(&read("report.pdf")), Some("read.docs"));
    // First-token tools: perl/ffmpeg run a binary → run.code; fd/readlink inspect
    // the filesystem → read.file; dig queries DNS → read.web; rustfmt formats →
    // lint.code; pdfinfo/pdftoppm read a PDF → read.docs; printenv/sysctl/uptime
    // report system state → read.proc.
    assert_eq!(tag(&bash("perl -pi -e 's/a/b/' f")), Some("run.code"));
    assert_eq!(tag(&bash("ffmpeg -i in.mp4 out.gif")), Some("run.code"));
    assert_eq!(tag(&bash("fd -e rs src")), Some("read.file"));
    assert_eq!(tag(&bash("readlink -f /x")), Some("read.file"));
    assert_eq!(tag(&bash("dig +short example.com")), Some("read.web"));
    assert_eq!(tag(&bash("rustfmt src/main.rs")), Some("lint.code"));
    assert_eq!(tag(&bash("pdfinfo doc.pdf")), Some("read.docs"));
    assert_eq!(tag(&bash("pdftoppm -png in.pdf out")), Some("read.docs"));
    assert_eq!(tag(&bash("printenv PATH")), Some("read.proc"));
    assert_eq!(tag(&bash("sysctl -n hw.ncpu")), Some("read.proc"));
    assert_eq!(tag(&bash("uptime")), Some("read.proc"));
    // git read-only subcommands surfaced by the loop.
    assert_eq!(
        tag(&bash("git diff-tree --no-commit-id -r HEAD")),
        Some("read.vcs")
    );
    assert_eq!(tag(&bash("git merge-base main HEAD")), Some("read.vcs"));
    assert_eq!(tag(&bash("git reflog")), Some("read.vcs"));
    assert_eq!(tag(&bash("git ls-remote origin")), Some("read.vcs"));
    assert_eq!(tag(&bash("git ls-tree HEAD")), Some("read.vcs"));
    assert_eq!(tag(&bash("git rev-list --count HEAD")), Some("read.vcs"));
    // pnpm build subcommand.
    assert_eq!(tag(&bash("pnpm build")), Some("build.code"));
    // Deferred multiplexers stay unmatched (surfaced for a future loop, not
    // force-tagged): aws/claude need per-subcommand mapping.
    assert_eq!(
        bash("aws sts get-caller-identity").disposition,
        TagDisposition::Unmatched
    );
    assert_eq!(
        bash("claude mcp list").disposition,
        TagDisposition::Unmatched
    );
}

// 2026-06-30 — Noise disposition. Shell-tokenizer fragments (vars, quotes,
// brackets, heredocs, regex, function defs, timestamps, non-ascii) were tried
// but the first token isn't a valid command identifier, so they're Noise, not
// Unmatched — the untagged loop stops resurfacing the same ~80 fragments every
// PR. Genuine unknown commands / multiplexers stay Unmatched (real candidates).
#[test]
fn noise_disposition_for_tokenizer_fragments() {
    for frag in [
        "}",
        "$page",
        "\"$soffice\" --headless",
        "<<'eof'",
        "strip(){ echo x; }",
        "[[pltv-optiflow]]",
        "^error\"",
        "true)",
        "fi)\"",
        "+%s)",
        "2026-05-14t10:11:00z",
    ] {
        assert_eq!(
            bash(frag).disposition,
            TagDisposition::Noise,
            "frag={frag:?}"
        );
        // Noise carries no taggable token (it must not feed the untagged loop).
        assert_eq!(bash(frag).value, None, "frag={frag:?}");
    }
    // Genuine unknown commands and multiplexers are valid identifiers → stay
    // Unmatched (real tagging candidates), NOT noise.
    assert_eq!(bash("frobnicate x").disposition, TagDisposition::Unmatched);
    assert_eq!(
        bash("git frobnicate").disposition,
        TagDisposition::Unmatched
    );
    assert_eq!(
        bash("aws sts get-caller-identity").disposition,
        TagDisposition::Unmatched
    );
    // Tagged / Control commands are unaffected.
    assert_eq!(bash("perl -pi -e x f").disposition, TagDisposition::Tagged);
    assert_eq!(bash("cd /tmp").disposition, TagDisposition::Control);
}

#[test]
fn display_is_meaningful_command_or_file_path() {
    assert_eq!(
        bash("cd /repo && git add -A && git status")
            .display
            .as_deref(),
        Some("git add -A && git status")
    );
    assert_eq!(
        read("/tmp/pr41.diff").display.as_deref(),
        Some("/tmp/pr41.diff")
    );
}

#[test]
fn no_dictionary_key_contains_slash() {
    // isPathExec가 먼저 돌므로 슬래시 포함 키는 도달 불가 — 사전 불변식.
    for (k, _) in BASH_FIRST_TOKEN_TAGS {
        assert!(!k.contains('/'), "{k}");
    }
    for (tool, subs) in TOOL_SUBCOMMAND_TAGS {
        assert!(!tool.contains('/'), "{tool}");
        for (s, _) in *subs {
            assert!(!s.contains('/'), "{tool} {s}");
        }
    }
}

// ── MCP 도구 태깅 (server→tool 멀티플렉서, "bash처럼 태그") ──────────────────

fn mcp(name: &str) -> TagOutcome {
    classify_tool_call(Some(name), &json!({ "input": {} }))
}

#[test]
fn mcp_serena_tools_tagged_by_verb() {
    assert_eq!(
        tag(&mcp("mcp__plugin_serena_serena__get_symbols_overview")),
        Some("read.code")
    );
    assert_eq!(
        tag(&mcp("mcp__plugin_serena_serena__find_symbol")),
        Some("read.code")
    );
    assert_eq!(
        tag(&mcp("mcp__plugin_serena_serena__search_for_pattern")),
        Some("read.code")
    );
    assert_eq!(
        tag(&mcp("mcp__plugin_serena_serena__replace_content")),
        Some("write.code")
    );
    assert_eq!(
        tag(&mcp("mcp__plugin_serena_serena__execute_shell_command")),
        Some("run.code")
    );
}

#[test]
fn mcp_token_is_server_scoped_for_the_loop() {
    // server name is the last underscore-segment of the server-id, so the
    // plugin-prefixed `plugin_serena_serena` and a bare `serena` both key on serena.
    let o = mcp("mcp__plugin_serena_serena__get_symbols_overview");
    assert_eq!(o.disposition, TagDisposition::Tagged);
    assert_eq!(o.token.as_deref(), Some("serena:get_symbols_overview"));
}

#[test]
fn mcp_serena_meta_tools_via_identification_loop() {
    // unidentified-plugins loop surfaced these (official → taggable):
    // initial_instructions reads serena's manual → read.docs.
    assert_eq!(
        tag(&mcp("mcp__plugin_serena_serena__initial_instructions")),
        Some("read.docs")
    );
    // get_current_config returns the agent config without mutating state → read.config.
    assert_eq!(
        tag(&mcp("mcp__plugin_serena_serena__get_current_config")),
        Some("read.config")
    );
    // activate_project(session setup·state write) and onboarding(meta) have no
    // code/file/docs verb → stay unmatched by design (surfaced, not force-tagged).
    assert_eq!(
        mcp("mcp__plugin_serena_serena__activate_project").disposition,
        TagDisposition::Unmatched
    );
    assert_eq!(
        mcp("mcp__plugin_serena_serena__onboarding").disposition,
        TagDisposition::Unmatched
    );
}

#[test]
fn mcp_official_connectors_tagged_by_prefix() {
    // claude-in-chrome (Anthropic browser extension): page actions → run.web,
    // reads → read.web, captures → write.image. Verb from the tool-name prefix.
    assert_eq!(
        tag(&mcp("mcp__claude-in-chrome__navigate")),
        Some("run.web")
    );
    assert_eq!(
        tag(&mcp("mcp__claude-in-chrome__computer")),
        Some("run.web")
    );
    assert_eq!(
        tag(&mcp("mcp__claude-in-chrome__browser_batch")),
        Some("run.web")
    );
    assert_eq!(
        tag(&mcp("mcp__claude-in-chrome__read_page")),
        Some("read.web")
    );
    assert_eq!(
        tag(&mcp("mcp__claude-in-chrome__get_page_text")),
        Some("read.web")
    );
    assert_eq!(tag(&mcp("mcp__claude-in-chrome__find")), Some("read.web"));
    assert_eq!(
        tag(&mcp("mcp__claude-in-chrome__tabs_context_mcp")),
        Some("read.web")
    );
    assert_eq!(
        tag(&mcp("mcp__claude-in-chrome__gif_creator")),
        Some("write.image")
    );

    // claude.ai Slack connector → read/write.chat.
    assert_eq!(
        tag(&mcp("mcp__claude_ai_Slack__slack_search_channels")),
        Some("read.chat")
    );
    assert_eq!(
        tag(&mcp("mcp__claude_ai_Slack__slack_read_thread")),
        Some("read.chat")
    );
    assert_eq!(
        tag(&mcp("mcp__claude_ai_Slack__slack_send_message")),
        Some("write.chat")
    );

    // claude.ai Linear connector → read/write.issue.
    assert_eq!(
        tag(&mcp("mcp__claude_ai_Linear__get_issue")),
        Some("read.issue")
    );
    assert_eq!(
        tag(&mcp("mcp__claude_ai_Linear__list_issues")),
        Some("read.issue")
    );
    assert_eq!(
        tag(&mcp("mcp__claude_ai_Linear__save_issue")),
        Some("write.issue")
    );
    assert_eq!(
        tag(&mcp("mcp__claude_ai_Linear__create_issue_label")),
        Some("write.issue")
    );

    // claude.ai Notion connector → read/write.docs (reuses the docs object).
    assert_eq!(
        tag(&mcp("mcp__claude_ai_Notion__notion-fetch")),
        Some("read.docs")
    );
    assert_eq!(
        tag(&mcp("mcp__claude_ai_Notion__notion-search")),
        Some("read.docs")
    );
    assert_eq!(
        tag(&mcp("mcp__claude_ai_Notion__notion-get-comments")),
        Some("read.docs")
    );
    assert_eq!(
        tag(&mcp("mcp__claude_ai_Notion__notion-create-pages")),
        Some("write.docs")
    );
    assert_eq!(
        tag(&mcp("mcp__claude_ai_Notion__notion-update-page")),
        Some("write.docs")
    );

    // token stays server-scoped (for the loop), even when tagged via prefix rule.
    let o = mcp("mcp__claude_ai_Slack__slack_send_message");
    assert_eq!(o.disposition, TagDisposition::Tagged);
    assert_eq!(o.token.as_deref(), Some("Slack:slack_send_message"));
}

#[test]
fn mcp_unknown_tool_is_unmatched_for_identification_loop() {
    // Known server, unknown tool → surfaced (unmatched), NOT control, so the
    // unidentified-plugin loop can pick it up. token is server-scoped.
    let o = mcp("mcp__plugin_serena_serena__some_brand_new_tool");
    assert_eq!(o.disposition, TagDisposition::Unmatched);
    assert_eq!(o.value, None);
    assert_eq!(o.token.as_deref(), Some("serena:some_brand_new_tool"));

    // A company/personal MCP server (not a universal official integration) has no
    // knowable-universal semantics → stays unmatched (we don't hardcode per-org
    // servers; only official integrations + marketplace plugins are tagged).
    let o2 = mcp("mcp__optiflow-help__optiflow_help_get_article");
    assert_eq!(o2.disposition, TagDisposition::Unmatched);
    assert_eq!(
        o2.token.as_deref(),
        Some("optiflow-help:optiflow_help_get_article")
    );
}

#[test]
fn non_mcp_control_tools_unchanged() {
    // TaskCreate etc. stay control (no verb.object tag) — unchanged by MCP work.
    let o = classify_tool_call(Some("TaskCreate"), &json!({ "input": { "subject": "x" } }));
    assert_eq!(o.disposition, TagDisposition::Control);
}

#[test]
fn mcp_dictionary_keys_well_formed() {
    // server keys carry no "__" (would break parsing); tool keys no slash.
    for (server, tools) in MCP_SERVER_TOOL_TAGS {
        assert!(!server.contains("__"), "{server}");
        for (t, _) in *tools {
            assert!(!t.contains('/') && !t.contains("__"), "{server} {t}");
        }
    }
}
