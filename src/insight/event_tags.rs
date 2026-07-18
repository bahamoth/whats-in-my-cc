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
    ("cs", "code"),
    ("mjs", "code"),
    ("md", "docs"),
    ("html", "docs"),
    ("txt", "docs"),
    ("pdf", "docs"),
    ("toml", "config"),
    ("yaml", "config"),
    ("yml", "config"),
    ("ini", "config"),
    ("conf", "config"),
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
    ("zsh", "code"),
    // tagging loop 2026-07-18 (release-distribution epic): cargo-insta
    // snapshot test files (`src/snapshots/*.snap`) are saved test-output —
    // diff/patch(data) 동족(저장된 출력을 다시 읽어들이는 텍스트).
    ("snap", "data"),
];

/// 확장자 없는 알려진 파일명 → object. `ext_of`가 빈 문자열을 주는 dotfile·
/// 무확장 파일(`.gitignore`/`justfile` 등)을 basename으로 조회한다
/// (2026-06-23 tagging loop). EXT_OBJECT(확장자) 미스 시에만 참조된다.
pub static FILENAME_OBJECT: &[(&str, &str)] = &[
    (".gitignore", "config"),
    ("justfile", "code"),
    // tagging loop 2026-07-03: .gitattributes는 .gitignore 동족, bare
    // `config`는 ~/.ssh/config·.git/config류의 보편 무확장 설정 파일명.
    (".gitattributes", "config"),
    ("config", "config"),
    // B-7 (2026-07-04): Makefile은 justfile 동족(빌드 스크립트 = code).
    ("Makefile", "code"),
];

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
    ("fd", "read.file"),
    ("ls", "read.file"),
    ("cat", "read.file"),
    ("head", "read.file"),
    ("tail", "read.file"),
    ("wc", "read.file"),
    ("jq", "read.file"),
    // base64는 jq/sed/awk와 같은 stdin 변환 계열 (tagging loop 2026-07-03).
    ("base64", "read.file"),
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
    ("readlink", "read.file"),
    ("diff", "read.file"),
    // read.proc — 프로세스/포트/시스템 상태
    ("ps", "read.proc"),
    ("lsof", "read.proc"),
    ("date", "read.proc"),
    ("printenv", "read.proc"),
    ("sysctl", "read.proc"),
    ("uptime", "read.proc"),
    // POSIX shell builtin — lists background jobs (ps/uptime peer, not
    // process control like pgrep/kill/wait in CONTROL_TOKENS) (tagging loop
    // 2026-07-05).
    ("jobs", "read.proc"),
    // process/system activity monitor — ps/uptime peer (tagging loop 2026-07-05).
    ("top", "read.proc"),
    // read.db
    ("sqlite3", "read.db"),
    ("psql", "read.db"),
    ("mysql", "read.db"),
    // read.web
    ("curl", "read.web"),
    ("wget", "read.web"),
    ("dig", "read.web"),
    // write.file — 생성/수정 (비파괴)
    ("mkdir", "write.file"),
    ("mktemp", "write.file"),
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
    ("perl", "run.code"),
    ("osascript", "run.code"),
    ("bash", "run.code"),
    ("sh", "run.code"),
    ("zsh", "run.code"),
    ("npx", "run.code"),
    ("markitdown", "run.code"),
    ("ffmpeg", "run.code"),
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
    ("rustfmt", "lint.code"),
    // misc single-purpose tools (2026-06-23 tagging loop)
    ("pdftotext", "read.docs"), // extract text from a PDF document
    ("pdfinfo", "read.docs"),   // PDF metadata / page info
    ("pdftoppm", "read.docs"),  // render PDF pages (reads the source PDF)
    ("serena", "run.code"),     // serena CLI (e.g. `serena project index`)
    // tagging loop 2026-07-04
    ("open", "run.proc"),      // macOS 앱/URL 실행 — 프로세스 기동
    ("plutil", "read.config"), // plist(설정) 조회/변환 (관측 표본은 `-o -` 조회)
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
    ("ls-tree", "read.vcs"),
    ("ls-remote", "read.vcs"),
    ("diff-tree", "read.vcs"),
    ("merge-base", "read.vcs"),
    ("reflog", "read.vcs"),
    ("rev-list", "read.vcs"),
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
    // tagging loop 2026-07-04: plumbing 조회.
    ("cat-file", "read.vcs"),
    ("for-each-ref", "read.vcs"),
];
static CARGO_SUBS: &[(&str, &str)] = &[
    ("build", "build.code"),
    ("b", "build.code"),
    ("test", "test.code"),
    ("t", "test.code"),
    ("nextest", "test.code"),
    ("run", "run.code"),
    ("r", "run.code"),
    // tagging loop 2026-07-10: cargo-watch(핫리로드 러너)는 로컬 프로젝트 코드를
    // 감시·재실행 — run 계열과 동족. just dev에 도입돼 관측됨.
    ("watch", "run.code"),
    ("check", "lint.code"),
    ("clippy", "lint.code"),
    ("fmt", "lint.code"),
    ("add", "write.deps"),
    ("update", "write.deps"),
    ("remove", "write.deps"),
    // tagging loop 2026-07-03: metadata/tree는 의존성 조회, clean은 빌드
    // 산출물 삭제(rm 동족), doc은 문서 생성.
    ("metadata", "read.deps"),
    ("tree", "read.deps"),
    ("clean", "delete.file"),
    ("doc", "build.docs"),
    // tagging loop 2026-07-18 (release-distribution epic): cargo install은
    // 바이너리 크레이트 설치 — npm install/cargo add 동족(write.deps). cargo
    // package는 관측 표본(전부 `--list --allow-dirty`)이 배포 전 포함 파일
    // 조회뿐이라 metadata/tree 동족(read.deps) — side-effect 있는 bare
    // `cargo package`(tarball 생성)는 미관측, 재표면화 시 세분화. cargo-insta
    // (snapshot testing 플러그인) accept는 관측 표본 전부 보류 스냅샷을
    // `.snap`(EXT_OBJECT: data) 파일에 반영하는 쓰기 동작 — write.data.
    ("install", "write.deps"),
    ("package", "read.deps"),
    ("insta", "write.data"),
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
    // tagging loop 2026-07-03: 의존성 조회.
    ("list", "read.deps"),
    ("ls", "read.deps"),
    // tagging loop 2026-07-18 (release-distribution epic): view는 registry
    // 메타데이터(버전 등) 조회 — list/ls 동족(read.deps).
    ("view", "read.deps"),
];
/// dist (구 cargo-dist, opensource.axo.dev/cargo-dist — 2026-07-18 "dist"로
/// 개명) — 릴리즈 CI/설치 스크립트 생성기 (release-distribution epic 도입).
/// generate/init은 `.github/workflows/*`·Cargo.toml `[workspace.metadata.dist]`
/// 설정을 다시 씀(write.config, chezmoi apply 동족), plan은 다음 릴리즈에서
/// 무엇이 될지 조회하는 dry-run(read.config, chezmoi diff 동족) — 부작용 없음.
/// 서브커맨드 없이 `--version`만 쓰인 관측 표본은 멀티플렉서 공통 규칙
/// (read.proc)으로 이미 처리된다.
static DIST_SUBS: &[(&str, &str)] = &[
    ("generate", "write.config"),
    ("init", "write.config"),
    ("plan", "read.config"),
];
/// launchctl — macOS launchd 제어 (release-distribution epic의 `wimcc
/// service` 구현·디버깅 중 관측). 관측 표본(n=3)은 전부 `print`(에이전트 상태
/// 조회) — ps/lsof 동족(read.proc). load/bootstrap/bootout 등 쓰기형 서브커맨드는
/// 미관측(해당 동작은 Rust 소스가 std::process::Command로 직접 호출하고
/// Bash 도구로는 나타나지 않음) — 재표면화 시 세분화.
static LAUNCHCTL_SUBS: &[(&str, &str)] = &[("print", "read.proc")];
/// rustup — 툴체인 관리 (tagging loop 2026-07-03). 설치·갱신은 write.deps,
/// show는 read.deps. `default` 등 미관측 서브커맨드는 unmatched로 남긴다.
static RUSTUP_SUBS: &[(&str, &str)] = &[
    ("toolchain", "write.deps"),
    ("component", "write.deps"),
    ("update", "write.deps"),
    ("install", "write.deps"),
    ("show", "read.deps"),
];
/// aws — sts는 자격 신원 조회(printenv/sysctl의 read.proc 동족). configure는
/// 로컬 자격/프로필 설정 조회(2026-07-09 추가, n=2 전부 `aws configure
/// list-profiles` — 로컬 ~/.aws 조회라 read.config). 그 외 서비스 서브커맨드는
/// verb가 3단계(aws <svc> <op>)에 있어 미관측인 채로 unmatched 유지
/// (tagging loop 2026-07-03).
static AWS_SUBS: &[(&str, &str)] = &[("sts", "read.proc"), ("configure", "read.config")];
/// chezmoi — dotfile 관리자 (tagging loop 2026-07-04). diff/managed/source-path는
/// 소스·타겟 상태 조회(read.config), apply는 타겟 반영(write.config). 미관측
/// 서브커맨드(add·edit 등)는 unmatched로 남긴다.
static CHEZMOI_SUBS: &[(&str, &str)] = &[
    ("diff", "read.config"),
    ("managed", "read.config"),
    ("source-path", "read.config"),
    ("apply", "write.config"),
];
/// volta — Node 툴체인 관리 (rustup install 동족, tagging loop 2026-07-04).
static VOLTA_SUBS: &[(&str, &str)] = &[("install", "write.deps")];
/// docker — 컨테이너 멀티플렉서 (tagging loop 2026-07-06). compose는 다중
/// 컨테이너 앱 실행 관리(공식 docs: "Compose is a tool for defining and
/// running multi-container applications") — 관측 표본(세션 c78d40d3, n=6)은
/// 전부 `compose up -d`(서비스 기동, open/`wimcc serve`의 run.proc 동족).
/// 2026-07-09 추가: ps는 컨테이너 목록/상태 조회(read.proc, n=10 전부
/// `ps --format …`), exec는 컨테이너 내 프로세스 실행(run.proc, n=13 전부
/// `exec … psql -c "select/update…"`). 그 외(build/rmi 등)는 미관측 —
/// unmatched로 남겨 재표면화 시 세분화한다.
static DOCKER_SUBS: &[(&str, &str)] = &[
    ("compose", "run.proc"),
    ("ps", "read.proc"),
    ("exec", "run.proc"),
];
/// claude — Claude Code CLI (tagging loop 2026-07-06). plugins는 플러그인
/// 레지스트리 조회(npm list의 read.deps 동족) — 관측 표본(세션 f6fa76f8,
/// n=4)은 전부 목록/도움말 조회(`plugins list --json`·`plugins marketplace
/// list`·`--help`). 설치형 op는 3단계라 미관측 — 재표면화 시 세분화.
static CLAUDE_SUBS: &[(&str, &str)] = &[("plugins", "read.deps")];
/// wimcc — 자기 CLI. 의미의 SSOT는 이 repo의 clap 정의(src/cli.rs):
/// init-db/ingest는 DB 쓰기, serve는 서버 기동, doctor는 환경 진단 조회
/// (tagging loop 2026-07-04; 보통 경로 실행 `./target/…/wimcc`(run.code)로
/// 관측되고 bare `wimcc`는 설치본 사용 시에만 나타난다).
static WIMCC_SUBS: &[(&str, &str)] = &[
    ("init-db", "write.db"),
    ("ingest", "write.db"),
    ("serve", "run.proc"),
    ("doctor", "read.proc"),
];
static PNPM_SUBS: &[(&str, &str)] = &[
    ("install", "write.deps"),
    ("i", "write.deps"),
    ("add", "write.deps"),
    ("test", "test.code"),
    ("vitest", "test.code"),
    ("build", "build.code"),
    ("start", "run.code"),
    ("run", "run.code"),
    // tagging loop 2026-07-04: dev-server 관행 스크립트(Vite/Next 표준 템플릿).
    // `exec`는 사전이 아니라 command_of의 투명 wrapper 처리, 그 외 미지 토큰은
    // classify_command의 pnpm 폴백(내부 토큰 재분류)이 담당.
    ("dev", "run.code"),
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
    ("rustup", RUSTUP_SUBS),
    ("aws", AWS_SUBS),
    ("chezmoi", CHEZMOI_SUBS),
    ("volta", VOLTA_SUBS),
    ("wimcc", WIMCC_SUBS),
    ("docker", DOCKER_SUBS),
    ("claude", CLAUDE_SUBS),
    ("dist", DIST_SUBS),
    ("launchctl", LAUNCHCTL_SUBS),
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
    // serena manual/config reads (unidentified-plugins loop, 2026-06-25/26).
    // `get_current_config` returns the agent config without mutating state (read).
    // `activate_project`(session setup·state write) and `onboarding`(meta) stay
    // intentionally unmatched — no code/file/docs verb fits.
    ("initial_instructions", "read.docs"),
    ("get_current_config", "read.config"),
];

/// MCP server → tool 사전(멀티플렉서). 미지 server/tool은 unmatched(루프가 표면화).
pub static MCP_SERVER_TOOL_TAGS: &[(&str, &[(&str, &str)])] = &[("serena", SERENA_TOOLS)];

/// 공식 통합(Anthropic 확장·claude.ai 커넥터)은 도구가 수십 개고 verb가 도구명
/// 접두에 안정적으로 드러난다 — 수십 개를 enumerate하는 대신 server별로 (접두→
/// "verb.object") + default를 둔다(object = 서버 도메인). exact 사전이 먼저 검사되어
/// 구체 override가 이긴다. "직접 설정한 one-off"가 아니라 동작이 알려진 보편 통합이라
/// 태깅 대상에 포함한다(회사/개인 전용 서버는 보편성이 없어 여전히 unmatched).
struct ConnectorRule {
    server: &'static str,
    /// 도구명 접두 → 전체 "verb.object"(static). 첫 매치가 이긴다.
    prefixes: &'static [(&'static str, &'static str)],
    /// 어느 접두에도 안 맞는 도구의 verb.object.
    default: &'static str,
}

static MCP_CONNECTOR_RULES: &[ConnectorRule] = &[
    // claude-in-chrome (Anthropic 브라우저 확장): 대부분 페이지 조작(run), 일부 read.
    ConnectorRule {
        server: "claude-in-chrome",
        prefixes: &[
            ("read_", "read.web"),
            ("get_", "read.web"),
            ("find", "read.web"),
            ("tabs_context", "read.web"),
            ("list_", "read.web"),
            ("gif_creator", "write.image"),
            ("upload_image", "write.image"),
            ("file_upload", "write.web"),
        ],
        default: "run.web", // navigate·computer·browser_batch·javascript·tabs_create/close·resize…
    },
    // claude.ai Slack 커넥터.
    ConnectorRule {
        server: "Slack",
        prefixes: &[("slack_search", "read.chat"), ("slack_read", "read.chat")],
        default: "write.chat", // send·schedule·draft·create_canvas·update_canvas
    },
    // claude.ai Linear 커넥터(이슈 트래커).
    ConnectorRule {
        server: "Linear",
        prefixes: &[
            ("get_", "read.issue"),
            ("list_", "read.issue"),
            ("search_", "read.issue"),
        ],
        default: "write.issue", // save·create·delete·update
    },
    // claude.ai Notion 커넥터(문서).
    ConnectorRule {
        server: "Notion",
        prefixes: &[
            ("notion-fetch", "read.docs"),
            ("notion-search", "read.docs"),
            ("notion-query", "read.docs"),
            ("notion-get", "read.docs"),
        ],
        default: "write.docs", // notion-create·update·duplicate·move
    },
    // context7 (공식 plugin, unidentified-plugins loop 2026-07-03): 도구 전부
    // 라이브러리 문서 조회 — query-docs·resolve-library-id.
    ConnectorRule {
        server: "context7",
        prefixes: &[],
        default: "read.docs",
    },
];

/// 공식 통합 서버의 도구를 접두 규칙으로 분류(exact 사전 miss 후 호출).
fn connector_tag(server: &str, tool: &str) -> Option<&'static str> {
    let rule = MCP_CONNECTOR_RULES.iter().find(|r| r.server == server)?;
    Some(
        rule.prefixes
            .iter()
            .find(|(p, _)| tool.starts_with(p))
            .map(|(_, v)| *v)
            .unwrap_or(rule.default),
    )
}

/// 서브커맨드보다 앞에 오는, 인자를 소비하는 글로벌 옵션 (`git -C <dir> diff`).
static SUBCOMMAND_ARG_FLAGS: &[(&str, &[&str])] = &[("git", &["-C", "-c"])];

static CONTROL_TOKENS: &[&str] = &[
    "cd", "echo", "printf", "sleep", "for", "export", "source", "set", "pgrep", "kill", "pkill",
    "wait", "true", ":", "while", "until", "if", "case", "esac", "done", "fi", "[", "[[", "test",
    "break", "continue", "exit",
    // B-7a 후속 (2026-07-04): `$(seq 1 N)` 루프 스캐폴딩 — 서브셸 내부가
    // 분류되면서 표면화된 순수 생성기. 작업 명령이 아니라 제어 반열.
    "seq",
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
    /// 분류를 시도했으나 첫 토큰이 유효한 명령 식별자가 아닌 셸 토크나이저
    /// 파편(변수·따옴표·괄호·heredoc·정규식·함수정의·타임스탬프·non-ascii 등).
    /// `Unmatched`와 달리 태깅 후보가 아니다 — untagged 루프(`collectUntagged`,
    /// `disposition === 'unmatched'`만 수집)에서 자동 제외돼 매 PR마다 같은
    /// 파편이 다시 표면화되지 않는다(2026-06-30).
    Noise,
}

impl TagDisposition {
    pub fn as_str(&self) -> &'static str {
        match self {
            TagDisposition::Tagged => "tagged",
            TagDisposition::Control => "control",
            TagDisposition::Unmatched => "unmatched",
            TagDisposition::Noise => "noise",
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
/// B-7a (2026-07-04) — `$( … )`/`<( … )` 명령 치환·프로세스 치환의 내부를
/// 별도 세그먼트로 뽑고, 바깥 명령에서는 자리표시자로 대체한다(내부 토큰
/// pr·rev-parse 류가 바깥 첫-토큰 분류를 오염시키지 않게). 중첩은 재귀
/// 편평화. 이스케이프(`\$(`)는 치환으로 보지 않는다. 따옴표는 기존
/// 토크나이저와 같은 수준으로 무시한다(quote-naive — 바깥 세그먼트가 먼저
/// 분류되므로 따옴표 안 오탐 내부는 실질적으로 표면화되지 않는다).
fn extract_command_subs(cmd: &str) -> (String, Vec<String>) {
    let chars: Vec<char> = cmd.chars().collect();
    let mut outer = String::new();
    let mut inners = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        let escaped = i > 0 && chars[i - 1] == '\\';
        let opens = !escaped
            && i + 1 < chars.len()
            && chars[i + 1] == '('
            && (chars[i] == '$' || chars[i] == '<');
        if opens {
            let mut depth = 1usize;
            let mut j = i + 2;
            while j < chars.len() && depth > 0 {
                match chars[j] {
                    '(' => depth += 1,
                    ')' => depth -= 1,
                    _ => {}
                }
                j += 1;
            }
            if depth == 0 {
                let inner: String = chars[i + 2..j - 1].iter().collect();
                inners.push(inner);
                // 유효 식별자 패턴 밖의 자리표시자 — 통째 서브셸 세그먼트는
                // Noise로 강등되고, 인자 위치에선 무해하다.
                outer.push_str("__cmdsub__");
                i = j;
                continue;
            }
        }
        outer.push(chars[i]);
        i += 1;
    }
    (outer, inners)
}

pub fn segment_command(cmd: &str) -> Vec<String> {
    let joined = join_continuations(cmd);
    let (outer, inners) = extract_command_subs(&joined);
    let outer_segs = split_separator_segments(&outer);
    // 서브셸 내부는 그것을 담은 바깥 세그먼트 "직후"에 스플라이스한다 —
    // 시간 순서 보존: `out=$(gh pr checks …)` 뒤의 `grep`보다 gh가 먼저
    // 분류된다(게이트 실측 2026-07-04, 세션 00fae5d9). 자리표시자 수로
    // 어느 세그먼트에서 나온 내부인지 순서 매핑한다.
    let mut inner_iter = inners.into_iter();
    let mut out: Vec<String> = Vec::new();
    for seg in outer_segs {
        let n = seg.matches("__cmdsub__").count();
        out.push(seg);
        for _ in 0..n {
            if let Some(inner) = inner_iter.next() {
                out.extend(segment_command(&inner));
            }
        }
    }
    for inner in inner_iter {
        out.extend(segment_command(&inner));
    }
    out
}

/// `&& || | ; &`·개행 구분자로 단순 분할 (segment_command의 분할부).
fn split_separator_segments(joined: &str) -> Vec<String> {
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

/// A first token that could never be a command we tag: it isn't a valid command
/// identifier (`^[a-z][a-z0-9._-]*$`). `first_token` already lowercased it, so
/// anything starting with a shell metachar/var/quote/bracket/digit, or carrying
/// `(){}[]<>$"'^+=|` etc., or non-ascii, is a tokenizer fragment — not a real
/// command. Callers map this to `Noise` (vs `Unmatched`) only when classification
/// already failed, so genuine multiplexers (`git`, `aws`) and unknown commands
/// (`frobnicate`) — all valid identifiers — stay `Unmatched` as tagging candidates.
fn is_noise_token(tok: &str) -> bool {
    let mut chars = tok.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() => {}
        _ => return true,
    }
    !tok.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
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
        if ft == "env" {
            // transparent wrapper (POSIX `env [VAR=val...] <cmd>`) — re-tag the
            // inner command. `is_assignment` above strips any `VAR=val` args on
            // the next loop iterations (tagging loop 2026-07-05, real sample:
            // `env WIMCC_PROXY_TARGET=… npx vite --port 5174`, this session's
            // own scratch-smoke-stack command).
            let rest = s["env".len()..].trim().to_string();
            if rest.is_empty() {
                return String::new();
            }
            s = rest;
            continue;
        }
        if ft == "command" {
            // POSIX builtin (tagging loop 2026-07-03): `command -v/-V X`는
            // 실행이 아니라 조회 — which의 의미 쌍둥이로 재작성해 사전이
            // read.file로 태깅하게 한다. 그 외(`command [-p] <cmd>`)는 함수
            // 우회 실행 wrapper — 내부 명령을 재분류.
            let mut rest = s["command".len()..].trim().to_string();
            if rest.starts_with("-v") || rest.starts_with("-V") {
                let args = rest[2..].trim();
                if args.is_empty() {
                    return String::new();
                }
                return format!("which {args}");
            }
            if let Some(stripped) = rest.strip_prefix("-p ") {
                rest = stripped.trim().to_string();
            }
            if rest.is_empty() {
                return String::new();
            }
            s = rest;
            continue;
        }
        if ft == "time" {
            // transparent wrapper (`time [-p] <cmd>`) — nohup과 동형, 선행
            // 플래그만 걷어낸다 (tagging loop 2026-07-03).
            let mut rest = s["time".len()..].trim().to_string();
            while rest.starts_with('-') {
                match rest.find(' ') {
                    None => {
                        rest = String::new();
                        break;
                    }
                    Some(sp) => rest = rest[sp + 1..].trim().to_string(),
                }
            }
            if rest.is_empty() {
                return String::new();
            }
            s = rest;
            continue;
        }
        if ft == "pnpm" {
            // `pnpm exec <cmd>` — 투명 wrapper (pnpm docs: 프로젝트 스코프에서
            // node_modules/.bin 명령 실행) — 내부 명령을 재분류한다
            // (tagging loop 2026-07-04). exec가 아니면 멀티플렉서 분류로 폴스루.
            if let Some(inner) = s["pnpm".len()..].trim().strip_prefix("exec ") {
                let inner = inner.trim().to_string();
                if inner.is_empty() {
                    return String::new();
                }
                s = inner;
                continue;
            }
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
    resolve_subcommand_with_tail(tool, rest).0
}

/// `resolve_subcommand` + 서브커맨드 토큰부터 시작하는 tail — pnpm 폴백이
/// 내부 명령(`<sub> <args…>`)을 재분류할 때 쓴다 (tagging loop 2026-07-04).
fn resolve_subcommand_with_tail<'a>(tool: &str, rest: &'a str) -> (String, &'a str) {
    let arg_flags: Option<&[&str]> = SUBCOMMAND_ARG_FLAGS
        .iter()
        .find(|(t, _)| *t == tool)
        .map(|(_, f)| *f);
    let mut skip_next = false;
    let mut chars = rest.char_indices().peekable();
    while let Some(&(start, c)) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }
        let mut end = rest.len();
        for (i, ch) in chars.by_ref() {
            if ch.is_whitespace() {
                end = i;
                break;
            }
        }
        let t = &rest[start..end];
        if skip_next {
            skip_next = false;
            continue;
        }
        if t.starts_with('+') {
            continue;
        }
        if t.starts_with('-') {
            skip_next = arg_flags.is_some_and(|f| f.contains(&t));
            continue;
        }
        return (t.to_lowercase(), &rest[start..]);
    }
    (String::new(), "")
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
        let rest = cmd_str[tok.len()..].trim_start();
        let (sub, sub_tail) = resolve_subcommand_with_tail(&tok, rest);
        let hit = subs.iter().find(|(k, _)| *k == sub).map(|(_, v)| *v);
        if let Some(v) = hit {
            return ClassifyResult {
                value: Some(v),
                disposition: TagDisposition::Tagged,
            };
        }
        // 서브커맨드 없이 `--version`만 — 도구 버전 조회는 시스템 상태 조회
        // (read.proc, printenv/sysctl 동족) (tagging loop 2026-07-04).
        if sub.is_empty() && rest.split_whitespace().any(|t| t == "--version") {
            return ClassifyResult {
                value: Some("read.proc"),
                disposition: TagDisposition::Tagged,
            };
        }
        // pnpm <script>/<binary> 폴백 (pnpm docs: built-in이 아닌 명령은
        // 스크립트/바이너리로 실행) — 내부 토큰이 사전 분류되면 그 태그를
        // 쓴다. 미분류면 기존대로 unmatched("pnpm <sub>"로 루프 표면화).
        if tok == "pnpm" && !sub.is_empty() {
            let inner = classify_command(sub_tail);
            if inner.disposition == TagDisposition::Tagged {
                return inner;
            }
        }
        return ClassifyResult {
            value: None,
            disposition: TagDisposition::Unmatched,
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
pub(crate) fn parse_mcp_tool(name: &str) -> Option<(&str, &str)> {
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
            // B-7 (2026-07-04): 확장자도 알려진 파일명도 없으면 내용 유형을
            // 특정할 수 없다 — 디렉터리/무확장 파일 탐색으로 보고 read.file
            // (ls/find 동족). 미지 "확장자"는 계속 unmatched(루프가 표면화).
            let value = read_tag_for_object(object_for_path(&fp, &e)).or(
                if e.is_empty() && !fp.is_empty() {
                    Some("read.file")
                } else {
                    None
                },
            );
            file_outcome(value, &fp, &e)
        }
        Some("Edit") | Some("Write") | Some("MultiEdit") => {
            let fp = str_field("file_path");
            let e = ext_of(&fp);
            // Read와 동형 — 무확장·미지 파일명 쓰기는 write.file.
            let value = write_tag_for_object(object_for_path(&fp, &e)).or(
                if e.is_empty() && !fp.is_empty() {
                    Some("write.file")
                } else {
                    None
                },
            );
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
                // Demote unmatched tokenizer fragments to Noise so the untagged
                // loop doesn't resurface them every PR (2026-06-30). Noise carries
                // no token — it is not a tagging candidate.
                if r.disposition == TagDisposition::Unmatched && is_noise_token(&tok) {
                    return TagOutcome {
                        value: None,
                        disposition: TagDisposition::Noise,
                        token: None,
                        display,
                    };
                }
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
                    .map(|(_, v)| *v)
                    // exact-dict miss → official-integration prefix rule (connectors).
                    .or_else(|| connector_tag(server, tool));
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
