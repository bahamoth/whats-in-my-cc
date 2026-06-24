//! Event tag 분류기 — webui `eventTags.ts`의 core 이전 (loop-foundations 2026-06-12).
//!
//! verb.object 어휘(read·write·delete·build·test·run·lint × code·docs·config·
//! data·file·proc·vcs·db·web·deps)로 tool_call을 결정론 분류한다. 이 분류는
//! lexical 측정이지 의미 판단이 아니다 — 분업 원칙상 wimcc core 소유이며,
//! UI와 MCP 소비자(LLM)가 같은 어휘를 본다. 종전에는 이 사전이 webui TS에만
//! 있어 MCP 소비자는 raw tool_name 밖에 보지 못했다(리뷰 발견 5).
//!
//! 패리티: `tests/event_tags.rs`가 webui `eventTags.test.ts`의 분류 케이스를
//! 1:1 이식해 잠근다. 규칙 추가(태깅 루프)는 이제 이 파일의 사전에 한다 —
//! 일반 첫 토큰은 `BASH_FIRST_TOKEN_TAGS`, 멀티플렉서 서브커맨드는
//! `TOOL_SUBCOMMAND_TAGS`, Read/Edit 확장자는 `EXT_OBJECT`.

use serde::Serialize;

// ── 사전 (single source of truth) ──────────────────────────────────────────

/// 파일 내용 유형(확장자 → object). Read는 `read.{object}`, Edit/Write는
/// `write.{object}`로 매핑된다.
pub static EXT_OBJECT: &[(&str, &str)] = &[
    ("rs", "code"),
    ("ts", "code"),
    ("tsx", "code"),
    ("js", "code"),
    ("jsx", "code"),
    ("css", "code"),
    ("py", "code"),
    ("vue", "code"),
    ("md", "docs"),
    ("html", "docs"),
    ("txt", "docs"),
    ("toml", "config"),
    ("yaml", "config"),
    ("yml", "config"),
    ("ini", "config"),
    ("json", "data"),
    ("sql", "data"),
    ("jsonl", "data"),
    ("log", "data"),
    ("csv", "data"),
    ("output", "data"),
    // saved diff/patch output text — written (saving) and read back as input.
    ("diff", "data"),
    ("patch", "data"),
    // image assets (2026-06-23 tagging loop) — read/written via Read/Edit.
    ("jpg", "image"),
    ("jpeg", "image"),
    ("png", "image"),
    ("gif", "image"),
    ("svg", "image"),
    ("webp", "image"),
    // shell script content is code (distinct from the `sh` COMMAND below).
    ("sh", "code"),
];

/// 확장자 없는 알려진 파일명 → object. `ext_of`가 빈 문자열을 주는 dotfile·
/// 무확장 파일(`.gitignore`/`justfile` 등)을 basename으로 조회한다
/// (2026-06-23 tagging loop). EXT_OBJECT(확장자) 미스 시에만 참조된다.
pub static FILENAME_OBJECT: &[(&str, &str)] = &[(".gitignore", "config"), ("justfile", "code")];

fn read_tag_for_object(object: &str) -> Option<&'static str> {
    match object {
        "code" => Some("read.code"),
        "docs" => Some("read.docs"),
        "config" => Some("read.config"),
        "data" => Some("read.data"),
        "image" => Some("read.image"),
        _ => None,
    }
}
fn write_tag_for_object(object: &str) -> Option<&'static str> {
    match object {
        "code" => Some("write.code"),
        "docs" => Some("write.docs"),
        "config" => Some("write.config"),
        "data" => Some("write.data"),
        "image" => Some("write.image"),
        _ => None,
    }
}

/// Bash 단일 목적 첫 토큰 → tag. 멀티플렉서(git/cargo/npm/…)는
/// `TOOL_SUBCOMMAND_TAGS`, 경로 직접 실행(`./x`, `/abs`, `*.sh`)은 run.code.
pub static BASH_FIRST_TOKEN_TAGS: &[(&str, &str)] = &[
    // read.file — 파일·디렉터리 탐색/검사
    ("grep", "read.file"),
    ("rg", "read.file"),
    ("egrep", "read.file"),
    ("fgrep", "read.file"),
    ("find", "read.file"),
    ("ls", "read.file"),
    ("cat", "read.file"),
    ("head", "read.file"),
    ("tail", "read.file"),
    ("wc", "read.file"),
    ("jq", "read.file"),
    ("tree", "read.file"),
    ("which", "read.file"),
    ("file", "read.file"),
    ("stat", "read.file"),
    ("du", "read.file"),
    ("df", "read.file"),
    ("sed", "read.file"),
    ("awk", "read.file"),
    ("pwd", "read.file"),
    ("realpath", "read.file"),
    ("diff", "read.file"),
    // read.proc — 프로세스/포트/시스템 상태
    ("ps", "read.proc"),
    ("lsof", "read.proc"),
    ("date", "read.proc"),
    // read.db
    ("sqlite3", "read.db"),
    ("psql", "read.db"),
    ("mysql", "read.db"),
    // read.web
    ("curl", "read.web"),
    ("wget", "read.web"),
    // write.file — 생성/수정 (비파괴)
    ("mkdir", "write.file"),
    ("touch", "write.file"),
    ("cp", "write.file"),
    ("chmod", "write.file"),
    ("chown", "write.file"),
    ("ln", "write.file"),
    // write.deps
    ("pip", "write.deps"),
    ("pip3", "write.deps"),
    // delete.file — 파괴적
    ("rm", "delete.file"),
    ("mv", "delete.file"),
    ("rmdir", "delete.file"),
    // run.code — 인터프리터/스크립트/패키지 바이너리
    ("python3", "run.code"),
    ("python", "run.code"),
    ("node", "run.code"),
    ("ruby", "run.code"),
    ("osascript", "run.code"),
    ("bash", "run.code"),
    ("sh", "run.code"),
    ("zsh", "run.code"),
    ("npx", "run.code"),
    ("markitdown", "run.code"),
    // read.file — 해시/검사
    ("shasum", "read.file"),
    ("sha256sum", "read.file"),
    ("md5", "read.file"),
    ("md5sum", "read.file"),
    // run.code — 태스크 러너
    ("just", "run.code"),
    // build / test / lint — 단일 목적 dev tool
    ("make", "build.code"),
    ("vitest", "test.code"),
    ("jest", "test.code"),
    ("pytest", "test.code"),
    ("eslint", "lint.code"),
    ("ruff", "lint.code"),
    ("prettier", "lint.code"),
    // misc single-purpose tools (2026-06-23 tagging loop)
    ("pdftotext", "read.docs"), // extract text from a PDF document
    ("serena", "run.code"),     // serena CLI (e.g. `serena project index`)
    // vcs (git 외)
    ("gh", "write.vcs"),
];

static GIT_SUBS: &[(&str, &str)] = &[
    ("status", "read.vcs"),
    ("log", "read.vcs"),
    ("diff", "read.vcs"),
    ("show", "read.vcs"),
    ("branch", "read.vcs"),
    ("blame", "read.vcs"),
    ("rev-parse", "read.vcs"),
    ("describe", "read.vcs"),
    ("fetch", "read.vcs"),
    ("remote", "read.vcs"),
    ("config", "read.vcs"),
    ("ls-files", "read.vcs"),
    ("check-ignore", "read.vcs"),
    ("shortlog", "read.vcs"),
    ("add", "write.vcs"),
    ("commit", "write.vcs"),
    ("push", "write.vcs"),
    ("checkout", "write.vcs"),
    ("switch", "write.vcs"),
    ("stash", "write.vcs"),
    ("rm", "write.vcs"),
    ("mv", "write.vcs"),
    ("reset", "write.vcs"),
    ("merge", "write.vcs"),
    ("rebase", "write.vcs"),
    ("pull", "write.vcs"),
    ("tag", "write.vcs"),
    ("clone", "write.vcs"),
    ("init", "write.vcs"),
    ("restore", "write.vcs"),
    ("cherry-pick", "write.vcs"),
    ("revert", "write.vcs"),
    ("apply", "write.vcs"),
    ("worktree", "write.vcs"),
];
static CARGO_SUBS: &[(&str, &str)] = &[
    ("build", "build.code"),
    ("b", "build.code"),
    ("test", "test.code"),
    ("t", "test.code"),
    ("nextest", "test.code"),
    ("run", "run.code"),
    ("r", "run.code"),
    ("check", "lint.code"),
    ("clippy", "lint.code"),
    ("fmt", "lint.code"),
    ("add", "write.deps"),
    ("update", "write.deps"),
    ("remove", "write.deps"),
];
static NPM_SUBS: &[(&str, &str)] = &[
    ("install", "write.deps"),
    ("i", "write.deps"),
    ("ci", "write.deps"),
    ("add", "write.deps"),
    ("test", "test.code"),
    ("t", "test.code"),
    ("start", "run.code"),
    ("run", "run.code"),
];
static PNPM_SUBS: &[(&str, &str)] = &[
    ("install", "write.deps"),
    ("i", "write.deps"),
    ("add", "write.deps"),
    ("test", "test.code"),
    ("vitest", "test.code"),
    ("start", "run.code"),
    ("run", "run.code"),
];
static YARN_SUBS: &[(&str, &str)] = &[
    ("install", "write.deps"),
    ("add", "write.deps"),
    ("test", "test.code"),
    ("start", "run.code"),
    ("run", "run.code"),
];
static GO_SUBS: &[(&str, &str)] = &[
    ("build", "build.code"),
    ("test", "test.code"),
    ("run", "run.code"),
    ("vet", "lint.code"),
    ("get", "write.deps"),
    ("install", "write.deps"),
];

/// 멀티플렉서: 서브커맨드가 verb를 결정. 미지의 서브커맨드는 unmatched(기본값
/// 없음) — 태깅 루프가 `tool sub`를 표면화해 여기 추가하게 한다.
pub static TOOL_SUBCOMMAND_TAGS: &[(&str, &[(&str, &str)])] = &[
    ("git", GIT_SUBS),
    ("cargo", CARGO_SUBS),
    ("npm", NPM_SUBS),
    ("pnpm", PNPM_SUBS),
    ("yarn", YARN_SUBS),
    ("go", GO_SUBS),
];

/// MCP 도구 태깅. 도구 이름은 `mcp__[plugin_<plugin>_]<server>__<tool>`. server→tool
/// 멀티플렉서로 verb.object를 매긴다(TOOL_SUBCOMMAND_TAGS와 동형). 미지의 server/tool은
/// Unmatched로 남겨 미식별-plugin 루프가 `server:tool`을 표면화하게 한다. 개인 제작
/// (personal) plugin·MCP는 여기 등록하지 않으므로 자동으로 태깅에서 빠진다 — provenance
/// 판별은 루프(파일시스템 접근)가 맡고, 결정론 분류기는 사전 매칭만 한다.
static SERENA_TOOLS: &[(&str, &str)] = &[
    ("get_symbols_overview", "read.code"),
    ("find_symbol", "read.code"),
    ("find_referencing_symbols", "read.code"),
    ("find_file", "read.code"),
    ("list_dir", "read.code"),
    ("read_file", "read.code"),
    ("search_for_pattern", "read.code"),
    ("get_diagnostics_for_file", "read.code"),
    ("replace_content", "write.code"),
    ("replace_symbol_body", "write.code"),
    ("insert_after_symbol", "write.code"),
    ("insert_before_symbol", "write.code"),
    ("rename_symbol", "write.code"),
    ("create_text_file", "write.code"),
    ("execute_shell_command", "run.code"),
    ("write_memory", "write.file"),
    ("read_memory", "read.file"),
    ("list_memories", "read.file"),
];

/// MCP server → tool 사전(멀티플렉서). 미지 server/tool은 unmatched(루프가 표면화).
pub static MCP_SERVER_TOOL_TAGS: &[(&str, &[(&str, &str)])] = &[("serena", SERENA_TOOLS)];

/// 서브커맨드보다 앞에 오는, 인자를 소비하는 글로벌 옵션 (`git -C <dir> diff`).
static SUBCOMMAND_ARG_FLAGS: &[(&str, &[&str])] = &[("git", &["-C", "-c"])];

static CONTROL_TOKENS: &[&str] = &[
    "cd", "echo", "printf", "sleep", "for", "export", "source", "set", "pgrep", "kill", "pkill",
    "wait", "true", ":", "while", "until", "if", "case", "esac", "done", "fi", "[", "[[", "test",
    "break", "continue",
];
static PREFIX_KEYWORDS: &[&str] = &["do", "then", "else", "elif"];
/// `timeout`의 값-소비 플래그 (공백 분리형 — `--signal=KILL`은 해당 없음).
static TIMEOUT_ARG_FLAGS: &[&str] = &["-s", "--signal", "-k", "--kill-after"];

// ── 결과 타입 ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TagDisposition {
    Tagged,
    Control,
    Unmatched,
}

impl TagDisposition {
    pub fn as_str(&self) -> &'static str {
        match self {
            TagDisposition::Tagged => "tagged",
            TagDisposition::Control => "control",
            TagDisposition::Unmatched => "unmatched",
        }
    }
}

/// tool_call 하나의 태그 분류 결과. API 응답(`events[].tag`)에 그대로 직렬화.
#[derive(Debug, Clone, Serialize)]
pub struct TagOutcome {
    /// `verb.object` 태그 — control/unmatched면 None.
    pub value: Option<&'static str>,
    pub disposition: TagDisposition,
    /// untagged 루프 집계 키: Bash 첫 토큰 | `"tool sub"` | 확장자 | basename.
    /// control(분류할 작업 없음)이면 None.
    pub token: Option<String>,
    /// 표시용 — 선행 제어 세그먼트를 제거한 명령 또는 file_path.
    pub display: Option<String>,
}

impl TagOutcome {
    fn control() -> Self {
        TagOutcome {
            value: None,
            disposition: TagDisposition::Control,
            token: None,
            display: None,
        }
    }
}

// ── 셸 파싱 (TS 구현 1:1) ──────────────────────────────────────────────────

fn first_token(cmd: &str) -> String {
    let t = cmd.trim();
    let end = t.find(' ').filter(|&sp| sp > 0).unwrap_or(t.len());
    t[..end].to_lowercase()
}

/// Resolve the content object for a file path: extension first (EXT_OBJECT),
/// then a basename fallback for known no-extension files (FILENAME_OBJECT).
/// Returns "" when neither matches (caller maps that to Unmatched).
fn object_for_path(fp: &str, ext: &str) -> &'static str {
    if let Some((_, o)) = EXT_OBJECT.iter().find(|(k, _)| *k == ext) {
        return o;
    }
    let base = fp.rsplit('/').next().unwrap_or("");
    FILENAME_OBJECT
        .iter()
        .find(|(k, _)| *k == base)
        .map(|(_, o)| *o)
        .unwrap_or("")
}

fn ext_of(path: &str) -> String {
    let base = path.rsplit('/').next().unwrap_or("");
    match base.rfind('.') {
        Some(i) if i > 0 => base[i + 1..].to_lowercase(),
        _ => String::new(),
    }
}

/// 줄 끝 `\` 계속행을 한 칸 공백으로 접는다 (newline 분할 전에 수행).
fn join_continuations(cmd: &str) -> String {
    let bytes = cmd.as_bytes();
    let mut out = String::with_capacity(cmd.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            // [ \t]* 백슬래시 \r? \n \s*  →  ' '
            let mut j = i + 1;
            if j < bytes.len() && bytes[j] == b'\r' {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'\n' {
                while out.ends_with(' ') || out.ends_with('\t') {
                    out.pop();
                }
                j += 1;
                while j < bytes.len() && (bytes[j] as char).is_whitespace() {
                    j += 1;
                }
                out.push(' ');
                i = j;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

/// `#`로 시작하는 줄 전체를 제거 (줄 중간 `#`는 문자열일 수 있어 보존).
fn strip_comment_lines(cmd: &str) -> String {
    cmd.lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// 시퀀서(&& · || · ; · | · 개행)로 복합 명령을 분할. 단독 `&`로는 나누지
/// 않는다(`2>&1` 보호).
pub fn segment_command(cmd: &str) -> Vec<String> {
    let joined = join_continuations(cmd);
    let bytes = joined.as_bytes();
    let mut segments = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        let sep_len = match bytes[i] {
            b'&' if i + 1 < bytes.len() && bytes[i + 1] == b'&' => 2,
            b'|' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'|' {
                    2
                } else {
                    1
                }
            }
            b';' | b'\n' => 1,
            _ => 0,
        };
        if sep_len > 0 {
            segments.push(joined[start..i].to_string());
            i += sep_len;
            start = i;
        } else {
            i += 1;
        }
    }
    segments.push(joined[start..].to_string());
    segments
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn is_assignment(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    for c in chars {
        if c == '=' {
            return true;
        }
        if !(c.is_ascii_alphanumeric() || c == '_') {
            return false;
        }
    }
    false
}

fn is_control_token(tok: &str) -> bool {
    CONTROL_TOKENS.contains(&tok) || tok.starts_with('-')
}

fn is_duration(tok: &str) -> bool {
    let t = tok.strip_suffix(['s', 'm', 'h', 'd']).unwrap_or(tok);
    !t.is_empty()
        && t.chars().all(|c| c.is_ascii_digit() || c == '.')
        && t.chars().filter(|&c| c == '.').count() <= 1
        && !t.starts_with('.')
        && !t.ends_with('.')
}

/// 세그먼트의 실제 명령 — 선행 `(`/`{`, `NAME=value` 할당, `do`/`then` 류
/// 접두 키워드, `timeout` 래퍼를 벗긴다. 명령이 안 남으면 빈 문자열.
fn command_of(segment: &str) -> String {
    let mut s = segment.trim().to_string();
    while s.starts_with('(') || s.starts_with('{') {
        s = s[1..].trim().to_string();
    }
    for _ in 0..12 {
        if is_assignment(&s) {
            match s.find(' ') {
                None => return String::new(),
                Some(sp) => {
                    s = s[sp + 1..].trim().to_string();
                    continue;
                }
            }
        }
        let ft = first_token(&s);
        if PREFIX_KEYWORDS.contains(&ft.as_str()) {
            match s.find(' ') {
                None => return String::new(),
                Some(sp) => {
                    s = s[sp + 1..].trim().to_string();
                    continue;
                }
            }
        }
        if ft == "nohup" {
            // transparent wrapper (`nohup <cmd>`) — re-tag the inner command.
            let rest = s["nohup".len()..].trim().to_string();
            if rest.is_empty() {
                return String::new();
            }
            s = rest;
            continue;
        }
        if ft == "timeout" {
            let mut rest = s["timeout".len()..].trim().to_string();
            while rest.starts_with('-') {
                let flag = first_token(&rest);
                match rest.find(' ') {
                    None => {
                        rest = String::new();
                        break;
                    }
                    Some(sp) => rest = rest[sp + 1..].trim().to_string(),
                }
                if TIMEOUT_ARG_FLAGS.contains(&flag.as_str()) {
                    match rest.find(' ') {
                        None => {
                            rest = String::new();
                            break;
                        }
                        Some(sp2) => rest = rest[sp2 + 1..].trim().to_string(),
                    }
                }
            }
            if is_duration(&first_token(&rest)) {
                rest = match rest.find(' ') {
                    None => String::new(),
                    Some(sp) => rest[sp + 1..].trim().to_string(),
                };
            }
            if rest.is_empty() {
                return String::new();
            }
            s = rest;
            continue;
        }
        return s;
    }
    s
}

/// 표시용 — 선행 제어 세그먼트(`cd … &&`)를 떼고 실제 작업부터 보여준다.
/// (개행은 표시 분할 대상이 아님 — TS 패리티.)
pub fn meaningful_command(cmd: &str) -> String {
    let mut s = join_continuations(cmd).trim().to_string();
    for _ in 0..6 {
        let Some((head, tail)) = split_at_first_separator(&s) else {
            break;
        };
        let head_cmd = command_of(head.trim());
        if !head_cmd.is_empty() && !is_control_token(&first_token(&head_cmd)) {
            break;
        }
        s = tail.trim().to_string();
    }
    if s.is_empty() {
        cmd.trim().to_string()
    } else {
        s
    }
}

/// `&&`·`||`·`;`·`|` 중 가장 왼쪽 구분자에서 1회 분할 (개행 제외).
fn split_at_first_separator(s: &str) -> Option<(&str, &str)> {
    let bytes = s.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'&' if i + 1 < bytes.len() && bytes[i + 1] == b'&' => {
                return Some((&s[..i], &s[i + 2..]));
            }
            b'|' => {
                let len = if i + 1 < bytes.len() && bytes[i + 1] == b'|' {
                    2
                } else {
                    1
                };
                return Some((&s[..i], &s[i + len..]));
            }
            b';' => return Some((&s[..i], &s[i + 1..])),
            _ => i += 1,
        }
    }
    None
}

/// 멀티플렉서의 실제 서브커맨드 — 선행 글로벌 옵션(`-x`/`--x`, 인자 소비형
/// `-C <dir>`)과 cargo `+toolchain`을 건너뛴다. 없으면 빈 문자열.
fn resolve_subcommand(tool: &str, rest: &str) -> String {
    let toks: Vec<&str> = rest.split_whitespace().collect();
    let arg_flags: Option<&[&str]> = SUBCOMMAND_ARG_FLAGS
        .iter()
        .find(|(t, _)| *t == tool)
        .map(|(_, f)| *f);
    let mut i = 0usize;
    while i < toks.len() {
        let t = toks[i];
        if t.starts_with('+') {
            i += 1;
            continue;
        }
        if t.starts_with('-') {
            i += if arg_flags.is_some_and(|f| f.contains(&t)) {
                2
            } else {
                1
            };
            continue;
        }
        break;
    }
    toks.get(i).map(|t| t.to_lowercase()).unwrap_or_default()
}

/// 명령 위치의 슬래시 포함 토큰은 실행 파일 — `./x`, `/abs/x`, `a/b`, `*.sh`.
/// 따옴표 시작 토큰은 heredoc/문자열 조각이라 제외.
fn is_path_exec(tok: &str) -> bool {
    if tok.ends_with(".sh") {
        return true;
    }
    if tok.starts_with('"') || tok.starts_with('\'') {
        return false;
    }
    tok.contains('/')
}

fn lookup_first_token(tok: &str) -> Option<&'static str> {
    BASH_FIRST_TOKEN_TAGS
        .iter()
        .find(|(k, _)| *k == tok)
        .map(|(_, v)| *v)
}
fn lookup_multiplexer(tok: &str) -> Option<&'static [(&'static str, &'static str)]> {
    TOOL_SUBCOMMAND_TAGS
        .iter()
        .find(|(k, _)| *k == tok)
        .map(|(_, v)| *v)
}

struct ClassifyResult {
    value: Option<&'static str>,
    disposition: TagDisposition,
}

/// (제어 접두가 제거된) 단일 명령 문자열 분류.
fn classify_command(cmd_str: &str) -> ClassifyResult {
    let tok = first_token(cmd_str);
    if tok.is_empty() || CONTROL_TOKENS.contains(&tok.as_str()) {
        return ClassifyResult {
            value: None,
            disposition: TagDisposition::Control,
        };
    }
    if is_path_exec(&tok) {
        return ClassifyResult {
            value: Some("run.code"),
            disposition: TagDisposition::Tagged,
        };
    }
    // tsc는 --noEmit이면 타입 체크(lint), 아니면 빌드.
    if tok == "tsc" {
        let value = if cmd_str.contains("--noEmit") {
            "lint.code"
        } else {
            "build.code"
        };
        return ClassifyResult {
            value: Some(value),
            disposition: TagDisposition::Tagged,
        };
    }
    if let Some(subs) = lookup_multiplexer(&tok) {
        let sub = resolve_subcommand(&tok, cmd_str[tok.len()..].trim_start());
        let hit = subs.iter().find(|(k, _)| *k == sub).map(|(_, v)| *v);
        return match hit {
            Some(v) => ClassifyResult {
                value: Some(v),
                disposition: TagDisposition::Tagged,
            },
            None => ClassifyResult {
                value: None,
                disposition: TagDisposition::Unmatched,
            },
        };
    }
    match lookup_first_token(&tok) {
        Some(v) => ClassifyResult {
            value: Some(v),
            disposition: TagDisposition::Tagged,
        },
        None => ClassifyResult {
            value: None,
            disposition: TagDisposition::Unmatched,
        },
    }
}

/// untagged 루프 집계 토큰 — 멀티플렉서는 `tool sub`, 그 외 첫 토큰.
fn untagged_token(cmd_str: &str) -> String {
    let tok = first_token(cmd_str);
    if lookup_multiplexer(&tok).is_some() {
        let sub = resolve_subcommand(&tok, cmd_str[tok.len()..].trim_start());
        if sub.is_empty() {
            tok
        } else {
            format!("{tok} {sub}")
        }
    } else {
        tok
    }
}

// ── 진입점 ─────────────────────────────────────────────────────────────────

/// tool_call 이벤트 하나를 분류한다. `payload`는 observed payload
/// (`/input/command` 또는 `/input/file_path`).
/// `mcp__[plugin_<plugin>_]<server>__<tool>` → (server_key, tool).
/// server_key = server-id의 마지막 `_` 세그먼트(plugin 접두의 밑줄 모호성 회피):
/// `plugin_serena_serena`→`serena`, `claude_ai_Slack`→`Slack`, `claude-in-chrome`→그대로.
fn parse_mcp_tool(name: &str) -> Option<(&str, &str)> {
    let rest = name.strip_prefix("mcp__")?;
    let idx = rest.find("__")?;
    let server_id = &rest[..idx];
    let tool = &rest[idx + 2..];
    if server_id.is_empty() || tool.is_empty() {
        return None;
    }
    let server_key = server_id.rsplit('_').next().unwrap_or(server_id);
    Some((server_key, tool))
}

pub fn classify_tool_call(tool_name: Option<&str>, payload: &serde_json::Value) -> TagOutcome {
    let input = payload.get("input");
    let str_field = |key: &str| -> String {
        input
            .and_then(|i| i.get(key))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };

    match tool_name {
        Some("Read") => {
            let fp = str_field("file_path");
            let e = ext_of(&fp);
            let value = read_tag_for_object(object_for_path(&fp, &e));
            file_outcome(value, &fp, &e)
        }
        Some("Edit") | Some("Write") | Some("MultiEdit") => {
            let fp = str_field("file_path");
            let e = ext_of(&fp);
            let value = write_tag_for_object(object_for_path(&fp, &e));
            file_outcome(value, &fp, &e)
        }
        Some("Bash") | Some("bash") => {
            let raw = str_field("command");
            let cmd = strip_comment_lines(&raw);
            if cmd.is_empty() {
                return TagOutcome::control();
            }
            let display = Some(meaningful_command(&raw));
            let segments = segment_command(&cmd);
            for seg in &segments {
                let cmd_str = command_of(seg);
                let tok = first_token(&cmd_str);
                if tok.is_empty() || is_control_token(&tok) {
                    continue;
                }
                let r = classify_command(&cmd_str);
                return TagOutcome {
                    value: r.value,
                    disposition: r.disposition,
                    token: Some(untagged_token(&cmd_str)),
                    display,
                };
            }
            TagOutcome {
                display,
                ..TagOutcome::control()
            }
        }
        // MCP 도구: server→tool 사전으로 verb.object. 미지는 unmatched(루프가 표면화).
        Some(name) if name.starts_with("mcp__") => match parse_mcp_tool(name) {
            Some((server, tool)) => {
                let value = MCP_SERVER_TOOL_TAGS
                    .iter()
                    .find(|(s, _)| *s == server)
                    .and_then(|(_, tools)| tools.iter().find(|(t, _)| *t == tool))
                    .map(|(_, v)| *v);
                TagOutcome {
                    value,
                    disposition: if value.is_some() {
                        TagDisposition::Tagged
                    } else {
                        TagDisposition::Unmatched
                    },
                    token: Some(format!("{server}:{tool}")),
                    display: None,
                }
            }
            None => TagOutcome::control(),
        },
        // 그 외 도구: 도구 이름이 곧 라벨 — 칩 없음.
        _ => TagOutcome::control(),
    }
}

fn file_outcome(value: Option<&'static str>, fp: &str, ext: &str) -> TagOutcome {
    let basename = fp.rsplit('/').next().unwrap_or("").to_string();
    let token = if ext.is_empty() {
        basename
    } else {
        ext.to_string()
    };
    TagOutcome {
        value,
        disposition: if value.is_some() {
            TagDisposition::Tagged
        } else {
            TagDisposition::Unmatched
        },
        token: Some(token),
        display: Some(fp.to_string()),
    }
}
