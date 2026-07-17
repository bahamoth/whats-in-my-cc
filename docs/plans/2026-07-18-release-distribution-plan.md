# 배포·릴리즈·업데이트 체계 구현 계획

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `docs/specs/2026-07-17-release-distribution.md` 구현 — dist 기반 4채널 배포(shell·brew·npm·crates.io) + `wimcc self-update` + 업데이트 체크/배너 + `wimcc service`.

**Architecture:** release-please는 현행 유지(태그·CHANGELOG·Release 생성), dist가 태그 트리거로 4타깃 빌드·installer·퍼블리시를 전담(`create-release = false`로 기존 Release에 업로드). 바이너리 쪽은 axoupdater(라이브러리)로 self-update, serve 백그라운드 태스크로 버전 체크, launchd/systemd로 서비스 등록.

**Tech Stack:** dist(구 cargo-dist, astral fork) · axoupdater 0.10 · semver 1 · 기존 스택(axum/tokio/reqwest-rustls/sqlx/insta, React/vitest).

## Global Constraints

- **운영 serve(:7878) 재시작·간섭 절대 금지.** 스모크는 스크래치 스택: `--db-path <scratch>.sqlite serve --port 7999 --auto-migrate` + `WIMCC_PROXY_TARGET=http://127.0.0.1:7999 npx vite --port 5174`.
- **TDD red 우선** — 실패 테스트 먼저, 빨강 확인 후 구현 (doc-only 커밋 예외).
- **커밋 footer 금지** — 프로젝트 hook이 `Co-Authored-By`/`Generated` footer를 차단한다. 넣지 말 것.
- **버전 파일 수동 수정 금지** — `Cargo.toml`·`webui/package.json`·`package-lock.json`의 version은 release-please 소관.
- **검증 출력 위생(R-20260706-3)** — 테스트·빌드·린트 출력은 grep/head/tail로 거르지 않는다. 무출력 도구는 `; echo EXIT=$?`만 덧붙인다.
- **1분+ 검증은 백그라운드(R-20260706-4)** — `cargo test` 전량, `clippy --all-targets`는 run_in_background.
- **WebUI 표기 원칙** — 판정 문장 금지(숫자·사실만), 보라 `#b07dff`는 코호트 경계 전용(배너에 사용 금지), 미측정은 `—`/null.
- **단일 PR** — 전 태스크를 이 브랜치(`feat/release-distribution`) 한 PR로. self-merge 금지(사용자가 rebase 머지).
- **Real-data anchoring** — GitHub API 등 외부 payload 의미 주장은 `tests/fixtures/**/real/` 동결 + invariant assertion으로 잠근다.

## 사전 준비물 (사용자 액션 — 구현과 병행, Task 9 전까지)

| # | 항목 | 확인 방법 |
|---|------|----------|
| 1 | npm 계정 + repo secret `NPM_TOKEN` | `gh secret list`에 표시 |
| 2 | crates.io 계정 + repo secret `CARGO_REGISTRY_TOKEN` | 〃 |
| 3 | `bahamoth/homebrew-tap` 저장소 생성(빈 repo) + 그 repo write 권한 PAT를 secret `HOMEBREW_TAP_TOKEN`으로 | 〃 |
| 4 | 태그 트리거용 PAT `RELEASE_PLEASE`(또는 `RELEASE_PLEASE_TOKEN`) 존재 확인 — **없으면 dist 워크플로가 태그에 반응하지 않는다**(`github.token`이 만든 태그는 워크플로를 트리거하지 않음) | 〃 (Task 2 Step 7에서 검증) |

---

### Task 1: 라이선스 + crates.io 패키징 메타데이터

**Files:**
- Create: `LICENSE-MIT`, `LICENSE-APACHE`, `scripts/check-crate-contents.sh`
- Modify: `Cargo.toml`(package 섹션), `.github/workflows/ci.yml`(rust job)

**Interfaces:**
- Produces: `scripts/check-crate-contents.sh`(exit 0/1 게이트 — CI와 Task 3 publish가 전제), `Cargo.toml`의 `include` 허용목록.

- [ ] **Step 1: 게이트 스크립트 작성 (red 먼저)**

`scripts/check-crate-contents.sh`:

```bash
#!/usr/bin/env bash
# crates.io 패키지에 컴파일 타임 임베드 자산이 동봉됐는지 게이트.
# rust-embed(webui/dist)·include_str!(pricing.json)·sqlx migrate!(migrations)가
# 빠지면 cargo install 사용자 쪽에서 빌드가 깨진다 — 스펙 §2.
set -euo pipefail
cd "$(dirname "$0")/.."
list=$(cargo package --list --allow-dirty)
missing=0
for path in webui/dist/index.html pricing.json migrations LICENSE-MIT LICENSE-APACHE; do
  if ! grep -q "$path" <<<"$list"; then
    echo "MISSING in crate: $path"
    missing=1
  fi
done
[ "$missing" -eq 0 ] && echo "crate contents OK"
exit "$missing"
```

`chmod +x scripts/check-crate-contents.sh`

- [ ] **Step 2: red 확인**

Run: `bash scripts/check-crate-contents.sh`
Expected: `MISSING in crate: webui/dist/index.html` 등 출력 후 exit 1 (`webui/dist`는 gitignore라 기본 패키징에서 빠짐). `webui/dist`가 없다면 먼저 `just webui-build`.

- [ ] **Step 3: LICENSE 파일 생성**

`LICENSE-APACHE`: `curl -fsSL https://www.apache.org/licenses/LICENSE-2.0.txt -o LICENSE-APACHE`

`LICENSE-MIT` (전문):

```text
MIT License

Copyright (c) 2026 bahamoth

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

- [ ] **Step 4: Cargo.toml [package] 확장**

`[package]`에 추가 (version 줄은 건드리지 않는다):

```toml
license = "MIT OR Apache-2.0"
repository = "https://github.com/bahamoth/whats-in-my-cc"
readme = "README.md"
# include는 허용목록으로 동작한다 — 여기 없는 파일은 .crate에서 빠진다.
# 컴파일 타임 임베드 3종(webui/dist·pricing.json·migrations)이 근거.
include = [
  "src/**",
  "migrations/**",
  "pricing.json",
  "webui/dist/**",
  "README.md",
  "LICENSE-MIT",
  "LICENSE-APACHE",
]
```

- [ ] **Step 5: green 확인**

Run: `bash scripts/check-crate-contents.sh`
Expected: `crate contents OK`, exit 0.

- [ ] **Step 6: CI에 게이트 추가**

`.github/workflows/ci.yml` rust job(`Rust (fmt + clippy + test)`)의 `Download webui-dist` 스텝 뒤에:

```yaml
      - name: crate packaging gate (webui/dist 동봉)
        run: bash scripts/check-crate-contents.sh
```

- [ ] **Step 7: 로컬 전체 확인 + 커밋**

Run: `cargo package --list --allow-dirty > /dev/null; echo EXIT=$?`
Expected: `EXIT=0` (전체 verify 빌드는 CI publish job이 수행 — 로컬에서는 목록 검증까지만)

```bash
git add LICENSE-MIT LICENSE-APACHE scripts/check-crate-contents.sh Cargo.toml .github/workflows/ci.yml
git commit -m "build(package): MIT OR Apache-2.0 라이선스 + crates.io include 허용목록·동봉 게이트"
```

---

### Task 2: dist 도입 — 4타깃 빌드·installer·퍼블리시 워크플로

**Files:**
- Create: `dist-workspace.toml`, `.github/build-setup.yml`, `.github/workflows/release.yml`(dist 생성물)
- Modify: `.github/workflows/release-please.yml`(upload job 제거), `Cargo.toml`(binstall 메타데이터)

**Interfaces:**
- Consumes: Task 1의 `include`(dist 자체는 안 쓰지만 같은 파일 수정).
- Produces: 릴리스 asset 네이밍(`wimcc-<target>.tar.xz` 예상 — Step 5에서 실측), Task 3 custom publish job 훅(`publish-jobs`), Task 5 self-update가 내려받는 릴리스 구조.

- [ ] **Step 1: dist CLI 설치**

```bash
curl -LsSf https://github.com/axodotdev/cargo-dist/releases/latest/download/cargo-dist-installer.sh | sh
dist --version
```

실패 시 폴백: `cargo install cargo-dist --locked`. 어느 쪽이든 `dist --version`이 출력돼야 진행.

- [ ] **Step 2: `dist init` 실행**

Run: `dist init --yes` (비대화식). `dist-workspace.toml`과 `.github/workflows/release.yml`이 생긴다.

- [ ] **Step 3: dist-workspace.toml을 확정 설정으로 교체**

`cargo-dist-version`은 init이 박은 값을 유지하고 나머지를 아래로:

```toml
[workspace]
members = ["cargo:."]

[dist]
# init이 기록한 cargo-dist-version 줄은 그대로 둔다
ci = "github"
installers = ["shell", "npm", "homebrew"]
targets = [
  "aarch64-apple-darwin",
  "x86_64-apple-darwin",
  "x86_64-unknown-linux-musl",
  "aarch64-unknown-linux-musl",
]
# release-please가 태그+Release를 만든다 — dist는 기존 Release에 업로드만 (스펙 §1)
create-release = false
tap = "bahamoth/homebrew-tap"
publish-jobs = ["homebrew", "npm", "./publish-crate"]
install-updater = false
# cargo 빌드 전 webui SPA 빌드 주입 (rust-embed 컴파일 타임 임베드 — 스펙 §1)
github-build-setup = "../build-setup.yml"

[dist.github-custom-runners]
aarch64-unknown-linux-musl = "ubuntu-24.04-arm"

[dist.dependencies.apt]
musl-tools = { version = "*", targets = ["x86_64-unknown-linux-musl", "aarch64-unknown-linux-musl"] }
```

`./publish-crate`는 Task 3에서 만든다(이 시점 `dist generate`가 워크플로 파일 부재로 실패하면 Task 3의 파일을 먼저 만들고 돌아온다).

- [ ] **Step 4: build-setup 스텝 파일 작성**

`.github/build-setup.yml` (dist가 `build-local-artifacts` job의 checkout 뒤에 주입):

```yaml
- name: Install Node
  uses: actions/setup-node@v4
  with:
    node-version: 22
    cache: npm
    cache-dependency-path: webui/package-lock.json
- name: Build webui (embedded by rust-embed at compile time)
  working-directory: webui
  run: |
    npm ci
    npm run build
```

- [ ] **Step 5: 워크플로 재생성 + 로컬 plan 검증**

```bash
dist generate
just webui-build
dist plan
```

Expected: `dist plan` 출력에 4개 타깃과 shell/npm/homebrew installer, `wimcc-<target>.tar.xz` 형태 asset 이름이 보인다. `.github/workflows/release.yml`에 ①`Install Node`/`Build webui` 주입 스텝 ②`aarch64-unknown-linux-musl` job의 `runs-on: ubuntu-24.04-arm` ③custom publish job 호출에 `secrets: inherit` — 3가지를 눈으로 확인. asset 이름이 예상과 다르면 Step 6의 `pkg-url`을 실측값으로 맞춘다.

- [ ] **Step 6: cargo-binstall 메타데이터 (실측 네이밍 반영)**

`Cargo.toml`에 추가:

```toml
[package.metadata.binstall]
# dist가 올리는 GitHub Releases 바이너리를 재사용 — binstall 사용자는 재컴파일하지 않는다
pkg-url = "{ repo }/releases/download/v{ version }/wimcc-{ target }.tar.xz"
pkg-fmt = "txz"
```

- [ ] **Step 7: release-please.yml 정리 + PAT 확인**

`.github/workflows/release-please.yml`에서 `upload-release-binaries` job 전체(25행의 주석 포함 26~73행)를 삭제한다. `release-please` job과 그 outputs는 유지.

Run: `gh secret list`
Expected: `RELEASE_PLEASE` 또는 `RELEASE_PLEASE_TOKEN`이 있어야 한다. **없으면 중단하고 사용자에게 PAT 등록을 요청** — 기본 `github.token`으로 만든 태그는 dist의 tag-push 트리거를 발화시키지 않는다.

- [ ] **Step 8: 커밋**

```bash
git add dist-workspace.toml .github/build-setup.yml .github/workflows/release.yml .github/workflows/release-please.yml Cargo.toml
git commit -m "ci(release): dist 도입 — 4타깃·shell/brew/npm installer, release-please는 태그·Release 생성 전담"
```

---

### Task 3: crates.io publish custom job

**Files:**
- Create: `.github/workflows/publish-crate.yml`

**Interfaces:**
- Consumes: Task 1 `include`·게이트 스크립트, Task 2 `publish-jobs = ["./publish-crate"]`.
- Produces: 릴리스 시 crates.io 자동 퍼블리시.

- [ ] **Step 1: 워크플로 작성**

`.github/workflows/publish-crate.yml` — dist 커스텀 publish job은 `workflow_call`로 호출되고 `secrets: inherit`를 받는다:

```yaml
name: publish-crate
on:
  workflow_call:

jobs:
  crates-io:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 22
          cache: npm
          cache-dependency-path: webui/package-lock.json
      # cargo package의 include가 webui/dist를 집으려면 퍼블리시 전에 빌드돼 있어야 한다
      - name: Build webui (bundled into the crate)
        working-directory: webui
        run: |
          npm ci
          npm run build
      - name: Install Rust toolchain
        run: rustup toolchain install
      - name: Packaging gate
        run: bash scripts/check-crate-contents.sh
      - name: cargo publish
        env:
          CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}
        run: cargo publish --locked --allow-dirty
```

(`--allow-dirty`: `webui/dist`는 gitignore된 미추적 산출물이라 원칙적으로 dirty가 아니지만, CI 생성 파일 변형에 방어적으로 둔다.)

- [ ] **Step 2: dist generate 재검증**

Run: `dist generate; echo EXIT=$?`
Expected: `EXIT=0`, `.github/workflows/release.yml`의 publish 단계에 `custom-publish-crate`(또는 유사 이름) job이 `uses: ./.github/workflows/publish-crate.yml` + `secrets: inherit`로 나타난다.

- [ ] **Step 3: 커밋**

```bash
git add .github/workflows/publish-crate.yml .github/workflows/release.yml
git commit -m "ci(release): crates.io publish custom job — webui 동봉 게이트 후 cargo publish"
```

---

### Task 4: 업데이트 체크 모듈 + `/v1/health` version 블록

**Files:**
- Create: `src/update_check.rs`, `tests/fixtures/update_check/real/releases_latest.json`
- Modify: `src/lib.rs`(mod 등록), `Cargo.toml`(semver), `src/cli.rs`(serve 플래그), `src/api/mod.rs`(AppState), `src/api/routes.rs`(health), `src/serve/mod.rs`(루프 spawn), `tests/api_health_insight.rs`, `docs/04_api_mcp_spec.html`

**Interfaces:**
- Produces:
  - `update_check::UpdateStatus { latest: Option<String>, update_available: bool }`, `type SharedUpdateStatus = Arc<tokio::sync::RwLock<UpdateStatus>>`
  - `update_check::is_newer(current: &str, latest_tag: &str) -> bool`
  - `update_check::parse_latest_tag(body: &str) -> Option<String>`
  - `update_check::run_update_check_loop(status: SharedUpdateStatus, url: String, shutdown: CancellationToken) -> impl Future`
  - `AppState.update_status: SharedUpdateStatus`
  - health 응답 `version: { current, latest, update_available }` — Task 6 배너가 소비.

- [ ] **Step 1: 실 fixture 채취 (Real-data anchoring)**

```bash
mkdir -p tests/fixtures/update_check/real
curl -s -H 'user-agent: wimcc-fixture-capture' -H 'accept: application/vnd.github+json' \
  https://api.github.com/repos/bahamoth/whats-in-my-cc/releases/latest \
  > tests/fixtures/update_check/real/releases_latest.json
head -c 200 tests/fixtures/update_check/real/releases_latest.json
```

Expected: `"tag_name": "v1.…"`가 포함된 JSON.

- [ ] **Step 2: 실패 테스트 작성**

`Cargo.toml` dependencies에 `semver = "1"` 추가. `src/update_check.rs` 생성 — 테스트만 먼저:

```rust
//! 새 버전 확인 — GitHub Releases 메타데이터 조회.
//! wimcc의 유일한 outbound 호출이다(스펙 §4). 실패는 조용히 무시하고 다음 주기에 재시도.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_patch_is_detected() {
        assert!(is_newer("1.3.0", "v1.3.1"));
        assert!(is_newer("1.3.0", "v2.0.0"));
    }

    #[test]
    fn same_or_older_is_not_newer() {
        assert!(!is_newer("1.3.0", "v1.3.0"));
        assert!(!is_newer("1.3.0", "v1.2.9"));
    }

    #[test]
    fn garbage_tag_is_not_newer() {
        assert!(!is_newer("1.3.0", "not-a-version"));
    }

    /// real fixture invariant: 태그는 v접두 semver (2026-07-18 채취).
    #[test]
    fn real_fixture_parses_v_prefixed_semver_tag() {
        let body = include_str!("../tests/fixtures/update_check/real/releases_latest.json");
        let tag = parse_latest_tag(body).expect("real payload has tag_name");
        assert!(tag.starts_with('v'), "tag was: {tag}");
        assert!(semver::Version::parse(tag.trim_start_matches('v')).is_ok());
    }
}
```

`src/lib.rs`에 `pub mod update_check;` 추가.

- [ ] **Step 3: red 확인**

Run: `cargo test --lib update_check 2>&1`
Expected: 컴파일 에러(`is_newer` 미정의) — red.

- [ ] **Step 4: 구현**

`src/update_check.rs` 상단(테스트 모듈 위)에:

```rust
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

/// 기본 조회처. 테스트·스모크는 `WIMCC_UPDATE_CHECK_URL`로 대체한다(코드 주석만, 문서화하지 않는 테스트용 노브).
pub const DEFAULT_LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/bahamoth/whats-in-my-cc/releases/latest";

/// 미조회/실패 = `latest: None` — 미측정을 0/거짓 양성으로 뭉개지 않는다(표기 원칙).
#[derive(Debug, Clone, Default)]
pub struct UpdateStatus {
    pub latest: Option<String>,
    pub update_available: bool,
}

pub type SharedUpdateStatus = Arc<RwLock<UpdateStatus>>;

#[derive(Debug, Deserialize)]
struct LatestRelease {
    tag_name: String,
}

pub fn parse_latest_tag(body: &str) -> Option<String> {
    serde_json::from_str::<LatestRelease>(body)
        .ok()
        .map(|r| r.tag_name)
}

pub fn is_newer(current: &str, latest_tag: &str) -> bool {
    let cur = semver::Version::parse(current.trim_start_matches('v'));
    let lat = semver::Version::parse(latest_tag.trim_start_matches('v'));
    match (cur, lat) {
        (Ok(c), Ok(l)) => l > c,
        _ => false,
    }
}

async fn fetch_latest(client: &reqwest::Client, url: &str) -> Option<String> {
    let resp = client
        .get(url)
        .header("user-agent", concat!("wimcc/", env!("CARGO_PKG_VERSION")))
        .header("accept", "application/vnd.github+json")
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    parse_latest_tag(&resp.text().await.ok()?)
}

/// serve 기동 시 spawn. tokio interval의 첫 tick은 즉시 발화하므로
/// "시작 시 + 24h 주기" 계약(스펙 §4)을 이 루프 하나로 충족한다.
pub async fn run_update_check_loop(
    status: SharedUpdateStatus,
    url: String,
    shutdown: CancellationToken,
) {
    let client = reqwest::Client::new();
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(60 * 60 * 24));
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => break,
            _ = interval.tick() => {
                if let Some(tag) = fetch_latest(&client, &url).await {
                    let newer = is_newer(env!("CARGO_PKG_VERSION"), &tag);
                    if newer {
                        tracing::info!(current = env!("CARGO_PKG_VERSION"), latest = %tag, "wimcc update available");
                    }
                    let mut s = status.write().await;
                    s.latest = Some(tag);
                    s.update_available = newer;
                }
            }
        }
    }
}
```

- [ ] **Step 5: green 확인**

Run: `cargo test --lib update_check 2>&1`
Expected: 4 passed.

- [ ] **Step 6: health 확장 실패 테스트**

`tests/api_health_insight.rs`에 추가:

```rust
/// 스펙 2026-07-17 §4 — health에 version 블록. 테스트 서버는 체크 루프를
/// 돌리지 않으므로 latest는 미조회 = null이어야 한다.
#[tokio::test]
async fn health_includes_version_block() {
    let srv = test_server().await;
    let body: serde_json::Value = srv.get("/v1/health").await.json();
    assert_eq!(body["version"]["current"], env!("CARGO_PKG_VERSION"));
    assert!(body["version"]["update_available"].is_boolean());
    assert!(body["version"]["latest"].is_null());
}
```

Run: `cargo test --test api_health_insight 2>&1`
Expected: `health_includes_version_block` FAILED — red.

- [ ] **Step 7: AppState + health + serve 배선**

1. `src/api/mod.rs` `AppState`에 필드 추가:

```rust
    /// 스펙 2026-07-17 §4 — 업데이트 체크 루프가 쓰고 health가 읽는다.
    pub update_status: crate::update_check::SharedUpdateStatus,
```

`cargo build`를 돌려 모든 `AppState { … }` 생성처(serve/mod.rs·테스트 헬퍼) 컴파일 에러에 `update_status: Default::default(),`를 추가한다.

2. `src/api/routes.rs::health`:

```rust
pub async fn health(State(state): State<AppState>) -> impl IntoResponse {
    let v = state.update_status.read().await;
    Json(json!({
        "status": "ok",
        "build_sha": option_env!("GIT_SHA").unwrap_or("dev"),
        "version": {
            "current": env!("CARGO_PKG_VERSION"),
            "latest": v.latest,
            "update_available": v.update_available,
        },
        "security": {
            "auth_required": !state.token.is_empty(),
            "retention_profile": state.retention_profile,
        }
    }))
}
```

3. `src/cli.rs` `Command::Serve`에 플래그 추가:

```rust
        /// 새 버전 자동 확인 — GitHub Releases 메타데이터 조회(유일한 outbound).
        /// off면 어떤 outbound도 발생하지 않는다. 스펙 2026-07-17 §4.
        #[arg(long, default_value = "on", value_parser = ["on", "off"], env = "WIMCC_UPDATE_CHECK")]
        update_check: String,
```

`src/cli.rs` 테스트 모듈에:

```rust
    #[test]
    fn serve_update_check_defaults_on() {
        let cli = Cli::try_parse_from(["wimcc", "serve"]).expect("parses");
        match cli.command {
            Command::Serve { update_check, .. } => assert_eq!(update_check, "on"),
            other => panic!("expected Serve, got {other:?}"),
        }
    }
```

4. `src/serve/mod.rs` — serve 기동부에서 (AppState 만든 뒤, 기존 백그라운드 태스크 spawn들과 같은 위치에):

```rust
    if update_check == "on" {
        let url = std::env::var("WIMCC_UPDATE_CHECK_URL")
            .unwrap_or_else(|_| crate::update_check::DEFAULT_LATEST_RELEASE_URL.to_string());
        tokio::spawn(crate::update_check::run_update_check_loop(
            state.update_status.clone(),
            url,
            state.shutdown.clone(),
        ));
    }
```

(`update_check` 인자는 main.rs의 Serve match arm에서 serve 진입 함수로 전달 — 기존 인자 전달 패턴을 따른다.)

- [ ] **Step 8: green 확인**

Run: `cargo test --test api_health_insight 2>&1` 그리고 `cargo test --lib cli 2>&1`
Expected: 전부 passed.

- [ ] **Step 9: API 스펙 문서 갱신**

`docs/04_api_mcp_spec.html`의 `/v1/health` 응답 예시에 version 블록을 반영:

```json
"version": { "current": "1.3.0", "latest": "v1.3.0", "update_available": false }
```

설명 한 줄: "latest는 GitHub Releases 조회 결과(24h 주기·`--update-check off`로 비활성) — 미조회 시 null."

- [ ] **Step 10: 커밋**

```bash
git add src/update_check.rs src/lib.rs src/cli.rs src/api/ src/serve/ src/main.rs Cargo.toml Cargo.lock tests/ docs/04_api_mcp_spec.html
git commit -m "feat(serve): 업데이트 체크 루프 + /v1/health version 블록 — 기본 on, --update-check off로 비활성"
```

---

### Task 5: `wimcc self-update` 서브커맨드

**Files:**
- Create: `src/self_update.rs`
- Modify: `Cargo.toml`(axoupdater), `src/lib.rs`, `src/cli.rs`, `src/main.rs`

**Interfaces:**
- Consumes: 없음(axoupdater 자체 완결 — Task 4의 is_newer를 쓰지 않는다).
- Produces: `self_update::run(check_only: bool) -> anyhow::Result<()>`, `self_update::decide(receipt_loaded: bool, receipt_is_for_this_exe: bool) -> Plan`, CLI `wimcc self-update [--check]`.

- [ ] **Step 1: 실패 테스트 작성**

`Cargo.toml`:

```toml
axoupdater = { version = "0.10", default-features = false, features = ["github_releases"] }
```

`src/self_update.rs` 생성 — 분기 로직 테스트 먼저:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// 스펙 §3 분기 계약: receipt가 있고 이 실행 파일의 것일 때만 실제 업데이트.
    #[test]
    fn shell_install_runs_update() {
        assert_eq!(decide(true, true), Plan::RunUpdate);
    }

    /// receipt 없음 = brew/npm/cargo 설치본 — 매니저 안내만.
    #[test]
    fn package_manager_install_is_guided() {
        assert_eq!(decide(false, false), Plan::ManagedElsewhere);
    }

    /// receipt는 있으나 다른 사본의 것(shell 설치본 receipt + brew 실행 파일).
    #[test]
    fn foreign_receipt_is_guided() {
        assert_eq!(decide(true, false), Plan::ManagedElsewhere);
    }
}
```

`src/cli.rs` 테스트 모듈에:

```rust
    #[test]
    fn self_update_parses_with_check_flag() {
        let cli = Cli::try_parse_from(["wimcc", "self-update", "--check"]).expect("parses");
        match cli.command {
            Command::SelfUpdate { check } => assert!(check),
            other => panic!("expected SelfUpdate, got {other:?}"),
        }
    }
```

`src/lib.rs`에 `pub mod self_update;` 추가.

- [ ] **Step 2: red 확인**

Run: `cargo test --lib self_update 2>&1`
Expected: 컴파일 에러(`decide`/`Plan` 미정의) — red.

- [ ] **Step 3: 구현**

`src/self_update.rs` 상단:

```rust
//! `wimcc self-update` — axoupdater 통합(스펙 2026-07-17 §3).
//! 불변: 어떤 경로에서도 실행 중 serve를 자동 재시작하지 않는다 —
//! serve 재기동은 라이브 CC 세션 관측을 중단시킬 수 있다.

use axoupdater::{AxoUpdater, ReleaseSource, ReleaseSourceType};

#[derive(Debug, PartialEq, Eq)]
pub enum Plan {
    /// shell installer 설치본(receipt 일치) — 실제 교체 수행.
    RunUpdate,
    /// 패키지 매니저 설치본 — 매니저 파일을 임의 교체하면 매니저 상태가 깨지므로 안내만.
    ManagedElsewhere,
}

pub fn decide(receipt_loaded: bool, receipt_is_for_this_exe: bool) -> Plan {
    if receipt_loaded && receipt_is_for_this_exe {
        Plan::RunUpdate
    } else {
        Plan::ManagedElsewhere
    }
}

pub async fn run(check_only: bool) -> anyhow::Result<()> {
    let mut updater = AxoUpdater::new_for("wimcc");
    let receipt_loaded = updater.load_receipt().is_ok();
    let receipt_matches =
        receipt_loaded && updater.check_receipt_is_for_this_executable().unwrap_or(false);

    if !receipt_loaded {
        // receipt가 없으면 source가 비어 --check도 못 하므로 GitHub를 직접 지정
        updater.set_release_source(ReleaseSource {
            release_type: ReleaseSourceType::GitHub,
            owner: "bahamoth".to_owned(),
            name: "whats-in-my-cc".to_owned(),
            app_name: "wimcc".to_owned(),
        });
        updater.set_current_version(env!("CARGO_PKG_VERSION").parse()?)?;
    }

    if check_only {
        if updater.is_update_needed().await? {
            println!("새 버전이 있습니다 — 현재 v{}", env!("CARGO_PKG_VERSION"));
        } else {
            println!("최신입니다 — v{}", env!("CARGO_PKG_VERSION"));
        }
        return Ok(());
    }

    match decide(receipt_loaded, receipt_matches) {
        Plan::ManagedElsewhere => {
            println!("이 wimcc는 패키지 매니저로 설치된 것으로 보입니다. 해당 매니저로 업데이트하세요:");
            println!("  brew upgrade wimcc | npm update -g wimcc | cargo install wimcc");
            Ok(())
        }
        Plan::RunUpdate => {
            match updater.run().await? {
                Some(result) => {
                    println!("업데이트 완료: {}", result.new_version_tag);
                    println!("실행 중인 serve는 구 바이너리로 계속 동작합니다.");
                    println!("라이브 CC 세션이 없을 때 재시작하세요: wimcc service restart (또는 수동 재기동)");
                }
                None => println!("이미 최신입니다 — v{}", env!("CARGO_PKG_VERSION")),
            }
            Ok(())
        }
    }
}
```

(axoupdater 0.10의 `set_current_version` 시그니처·`UpdateResult` 필드명이 다르면 컴파일 에러를 따라 맞추되, receipt 분기·안내 문구·자동 재시작 금지는 계약대로 유지한다.)

`src/cli.rs` `Command`에:

```rust
    /// 바이너리를 최신 릴리스로 교체한다. 실행 중인 serve는 재시작하지 않는다.
    SelfUpdate {
        /// 조회만 하고 교체하지 않는다.
        #[arg(long)]
        check: bool,
    },
```

`src/main.rs` match에 arm 추가(기존 arm 패턴대로):

```rust
        Command::SelfUpdate { check } => wimcc::self_update::run(check).await?,
```

- [ ] **Step 4: green 확인**

Run: `cargo test --lib self_update 2>&1` 그리고 `cargo test --lib cli 2>&1`
Expected: 전부 passed (신규 4개 포함).

- [ ] **Step 5: 수동 스모크**

Run: `cargo run -- self-update --check 2>&1`
Expected: 개발 빌드(receipt 없음)에서 GitHub 조회 후 "새 버전이 있습니다/최신입니다" 한 줄. `cargo run -- self-update 2>&1` → 패키지 매니저 안내 출력(receipt 없음 분기).

- [ ] **Step 6: 커밋**

```bash
git add src/self_update.rs src/lib.rs src/cli.rs src/main.rs Cargo.toml Cargo.lock
git commit -m "feat(cli): wimcc self-update — axoupdater receipt 분기, 자동 재시작 금지"
```

---

### Task 6: WebUI 업데이트 배너

**Files:**
- Create: `webui/src/components/layout/UpdateBanner.tsx`, `webui/src/components/layout/UpdateBanner.module.css`, `webui/src/components/layout/__tests__/UpdateBanner.test.tsx`
- Modify: `webui/src/api/types.ts`, `webui/src/api/client.ts`, `webui/src/components/layout/AppShell.tsx`, `webui/src/i18n/catalog/en.ts`, `webui/src/i18n/catalog/ko.ts`

**Interfaces:**
- Consumes: Task 4의 health `version` 블록.
- Produces: `getHealthVersion(): Promise<HealthVersion | null>`, `<UpdateBanner />`.

- [ ] **Step 1: 실패 테스트 작성**

`webui/src/components/layout/__tests__/UpdateBanner.test.tsx` (같은 폴더 기존 테스트의 import·렌더 헬퍼 관례를 따른다 — i18n Provider 래핑 포함):

```tsx
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { UpdateBanner } from '../UpdateBanner';

const mockGet = vi.hoisted(() => vi.fn());
vi.mock('../../../api/client', () => ({ getHealthVersion: mockGet }));

describe('UpdateBanner', () => {
  it('update_available=true면 두 버전 숫자를 표기한다', async () => {
    mockGet.mockResolvedValue({ current: '1.3.0', latest: 'v9.9.9', update_available: true });
    render(<UpdateBanner />);
    await waitFor(() => {
      expect(screen.getByRole('status').textContent).toContain('v9.9.9');
      expect(screen.getByRole('status').textContent).toContain('v1.3.0');
    });
  });

  it('update_available=false면 렌더하지 않는다', async () => {
    mockGet.mockResolvedValue({ current: '1.3.0', latest: 'v1.3.0', update_available: false });
    const { container } = render(<UpdateBanner />);
    await waitFor(() => expect(mockGet).toHaveBeenCalled());
    expect(container.firstChild).toBeNull();
  });

  it('닫기 버튼으로 배너를 숨긴다', async () => {
    mockGet.mockResolvedValue({ current: '1.3.0', latest: 'v9.9.9', update_available: true });
    render(<UpdateBanner />);
    await waitFor(() => expect(screen.getByRole('status')).toBeInTheDocument());
    await userEvent.click(screen.getByRole('button'));
    expect(screen.queryByRole('status')).toBeNull();
  });
});
```

(i18n Provider가 필요해 render가 깨지면 기존 layout 테스트의 래퍼를 그대로 사용.)

- [ ] **Step 2: red 확인**

Run: `cd webui && npx vitest run src/components/layout/__tests__/UpdateBanner.test.tsx 2>&1`
Expected: FAIL (`UpdateBanner` 미존재) — red.

- [ ] **Step 3: 구현**

`webui/src/api/types.ts`에:

```ts
/** /v1/health의 version 블록 — 스펙 2026-07-17 §4. latest 미조회 = null. */
export interface HealthVersion {
  current: string;
  latest: string | null;
  update_available: boolean;
}
```

`webui/src/api/client.ts`에 (health는 Envelope 미사용 — `routes.rs::health`가 원시 JSON 반환):

```ts
export async function getHealthVersion(): Promise<HealthVersion | null> {
  try {
    const resp = await fetch('/v1/health', { headers: { accept: 'application/json' } });
    if (!resp.ok) return null; // auth on 등 — 배너는 조용히 생략
    const body = await resp.json();
    return (body.version as HealthVersion | undefined) ?? null;
  } catch {
    return null;
  }
}
```

(import 목록에 `HealthVersion` 추가.)

i18n — `webui/src/i18n/catalog/en.ts`:

```ts
  // Update banner — 버전 숫자·사실만 (판정 문장 금지)
  'update.available': (current: string, latest: string) =>
    `wimcc ${latest} available — current v${current}`,
  'update.releaseNotes': 'Release notes',
  'update.dismiss': 'Dismiss update notice',
```

`webui/src/i18n/catalog/ko.ts`:

```ts
  // Update banner — 버전 숫자·사실만 (판정 문장 금지)
  'update.available': (current: string, latest: string) =>
    `wimcc ${latest} 사용 가능 — 현재 v${current}`,
  'update.releaseNotes': '릴리스 노트',
  'update.dismiss': '업데이트 알림 닫기',
```

`webui/src/components/layout/UpdateBanner.tsx`:

```tsx
import { useEffect, useState } from 'react';
import { useT } from '../../i18n';
import { getHealthVersion } from '../../api/client';
import type { HealthVersion } from '../../api/types';
import styles from './UpdateBanner.module.css';

/** 새 릴리스가 있을 때만 뜨는 시스템 배너. 세션당 1회 닫기 가능(영속 없음 — YAGNI). */
export function UpdateBanner() {
  const t = useT();
  const [version, setVersion] = useState<HealthVersion | null>(null);
  const [dismissed, setDismissed] = useState(false);

  useEffect(() => {
    void getHealthVersion().then(setVersion);
  }, []);

  if (dismissed || !version?.update_available || !version.latest) return null;
  return (
    <div className={styles.banner} role="status">
      <span>{t('update.available')(version.current, version.latest)}</span>
      <a
        className={styles.link}
        href={`https://github.com/bahamoth/whats-in-my-cc/releases/tag/${version.latest}`}
        target="_blank"
        rel="noreferrer"
      >
        {t('update.releaseNotes')}
      </a>
      <button
        type="button"
        className={styles.dismiss}
        onClick={() => setDismissed(true)}
        aria-label={t('update.dismiss')}
      >
        ×
      </button>
    </div>
  );
}
```

`webui/src/components/layout/UpdateBanner.module.css` (보라 `#b07dff`는 코호트 경계 전용이므로 금지 — 중립 블루 액센트):

```css
.banner {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  padding: 0.4rem 0.9rem;
  font-size: 0.85rem;
  border-bottom: 1px solid var(--border, #2c3140);
  background: color-mix(in srgb, #3b82f6 12%, transparent);
}
.link {
  text-decoration: underline;
}
.dismiss {
  margin-left: auto;
  background: none;
  border: none;
  cursor: pointer;
  font-size: 1rem;
  color: inherit;
}
```

`AppShell.tsx` — `<main>` 첫 자식으로:

```tsx
      <main id="wimcc-main" className={styles.main} role="main">
        <UpdateBanner />
        {children}
      </main>
```

(+ `import { UpdateBanner } from './UpdateBanner';`)

- [ ] **Step 4: green 확인**

Run: `cd webui && npx vitest run 2>&1`
Expected: 전체 passed (신규 3개 포함, tipStyle 게이트 포함 — 신규 키는 `.tip`이 아니므로 대상 아님).

- [ ] **Step 5: 브라우저 스모크 (UI 원칙 — commit 전 필수)**

```bash
# 가짜 최신 릴리스 fixture로 배너 강제 발화
mkdir -p /tmp/wimcc-banner-smoke && cd /tmp/wimcc-banner-smoke
echo '{"tag_name":"v99.0.0"}' > releases_latest.json
python3 -m http.server 8099 &   # 스모크 후 kill
# 스크래치 serve (운영 :7878 금지)
WIMCC_UPDATE_CHECK_URL=http://127.0.0.1:8099/releases_latest.json \
  cargo run -- --db-path /tmp/wimcc-banner-smoke/scratch.sqlite serve --port 7999 --auto-migrate &
cd /Users/bahamoth/projects/whats-in-my-cc/webui && WIMCC_PROXY_TARGET=http://127.0.0.1:7999 npx vite --port 5174
```

브라우저에서 `http://127.0.0.1:5174` 확인: ① 배너에 `wimcc v99.0.0 사용 가능 — 현재 v1.3.0` ② 릴리스 노트 링크 href ③ × 닫기 동작 ④ override 없이 재기동하면 배너 없음. 확인 후 스모크 프로세스 정리.

- [ ] **Step 6: 커밋**

```bash
git add webui/src/api/ webui/src/components/layout/ webui/src/i18n/catalog/
git commit -m "feat(webui): 업데이트 배너 — health version 블록 소비, 버전 숫자만 표기"
```

---

### Task 7: `wimcc service` 서브커맨드

**Files:**
- Create: `src/service.rs`
- Modify: `src/lib.rs`, `src/cli.rs`, `src/main.rs`

**Interfaces:**
- Consumes: 없음.
- Produces: `service::launchd_plist(argv: &[String]) -> String`, `service::systemd_unit(argv: &[String]) -> String`, `service::run(action: ServiceAction, db_path: &Path) -> anyhow::Result<()>`, CLI `wimcc service install|uninstall|restart|status`.

- [ ] **Step 1: 실패 스냅샷 테스트 작성**

`src/service.rs` 생성 — 테스트 먼저:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn argv() -> Vec<String> {
        [
            "/usr/local/bin/wimcc",
            "--db-path",
            "/data/wimcc.sqlite",
            "serve",
            "--bind",
            "127.0.0.1",
            "--port",
            "7878",
            "--auto-migrate",
        ]
        .map(String::from)
        .to_vec()
    }

    #[test]
    fn launchd_plist_snapshot() {
        insta::assert_snapshot!(launchd_plist(&argv()));
    }

    #[test]
    fn systemd_unit_snapshot() {
        insta::assert_snapshot!(systemd_unit(&argv()));
    }
}
```

`src/lib.rs`에 `pub mod service;` 추가.

- [ ] **Step 2: red 확인**

Run: `cargo test --lib service 2>&1`
Expected: 컴파일 에러(함수 미정의) — red.

- [ ] **Step 3: 구현**

`src/service.rs` 상단:

```rust
//! `wimcc service` — serve를 OS 사용자 서비스로 등록(스펙 2026-07-17 §5).
//! macOS launchd(gui 도메인) + Linux systemd user unit. 실행 주체는 항상 사용자.

use anyhow::{bail, Context};
use std::path::{Path, PathBuf};
use std::process::Command as Proc;

pub const SERVICE_LABEL: &str = "com.bahamoth.wimcc";

/// argv[0]=실행 파일 절대경로, 이후 전체 인자. 서비스는 CWD 보장이 없으므로
/// 경로 인자는 호출부에서 절대경로로 만들어 전달한다.
pub fn launchd_plist(argv: &[String]) -> String {
    let items: String = argv
        .iter()
        .map(|a| format!("    <string>{a}</string>\n"))
        .collect();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{SERVICE_LABEL}</string>
  <key>ProgramArguments</key>
  <array>
{items}  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <true/>
</dict>
</plist>
"#
    )
}

pub fn systemd_unit(argv: &[String]) -> String {
    let exec: String = argv
        .iter()
        .map(|a| format!("\"{a}\""))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        r#"[Unit]
Description=wimcc serve — local Claude Code observation

[Service]
ExecStart={exec}
Restart=on-failure

[Install]
WantedBy=default.target
"#
    )
}
```

이어서 운영부(스냅샷 대상 아님):

```rust
fn plist_path() -> anyhow::Result<PathBuf> {
    Ok(dirs::home_dir()
        .context("no home dir")?
        .join("Library/LaunchAgents")
        .join(format!("{SERVICE_LABEL}.plist")))
}

fn unit_path() -> anyhow::Result<PathBuf> {
    Ok(dirs::home_dir()
        .context("no home dir")?
        .join(".config/systemd/user/wimcc.service"))
}

fn current_uid() -> anyhow::Result<String> {
    let out = Proc::new("id").arg("-u").output().context("id -u")?;
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn run_cmd(program: &str, args: &[&str]) -> anyhow::Result<bool> {
    let status = Proc::new(program)
        .args(args)
        .status()
        .with_context(|| format!("{program} 실행 실패"))?;
    Ok(status.success())
}

fn build_argv(db_path: &Path, bind: &str, port: u16, auto_migrate: bool) -> anyhow::Result<Vec<String>> {
    let exe = std::env::current_exe().context("current_exe")?;
    // 서비스는 홈 디렉터리 CWD로 돌므로 상대 db 경로는 절대화한다
    let db_abs = if db_path.is_absolute() {
        db_path.to_path_buf()
    } else {
        std::env::current_dir()?.join(db_path)
    };
    let mut argv = vec![
        exe.to_string_lossy().into_owned(),
        "--db-path".into(),
        db_abs.to_string_lossy().into_owned(),
        "serve".into(),
        "--bind".into(),
        bind.into(),
        "--port".into(),
        port.to_string(),
    ];
    if auto_migrate {
        argv.push("--auto-migrate".into());
    }
    Ok(argv)
}

pub fn install(db_path: &Path, bind: &str, port: u16, auto_migrate: bool) -> anyhow::Result<()> {
    let argv = build_argv(db_path, bind, port, auto_migrate)?;
    if cfg!(target_os = "macos") {
        let path = plist_path()?;
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(&path, launchd_plist(&argv))?;
        let uid = current_uid()?;
        run_cmd("launchctl", &["bootstrap", &format!("gui/{uid}"), &path.to_string_lossy()])?;
        println!("등록 완료: {}", path.display());
    } else if cfg!(target_os = "linux") {
        let path = unit_path()?;
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(&path, systemd_unit(&argv))?;
        run_cmd("systemctl", &["--user", "daemon-reload"])?;
        run_cmd("systemctl", &["--user", "enable", "--now", "wimcc"])?;
        println!("등록 완료: {}", path.display());
    } else {
        bail!("지원하지 않는 OS — macOS(launchd)·Linux(systemd)만");
    }
    println!("로그인 시 serve가 자동 시작됩니다. 해제: wimcc service uninstall");
    Ok(())
}

pub fn uninstall() -> anyhow::Result<()> {
    if cfg!(target_os = "macos") {
        let uid = current_uid()?;
        run_cmd("launchctl", &["bootout", &format!("gui/{uid}/{SERVICE_LABEL}")])?;
        let path = plist_path()?;
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
    } else if cfg!(target_os = "linux") {
        run_cmd("systemctl", &["--user", "disable", "--now", "wimcc"])?;
        let path = unit_path()?;
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        run_cmd("systemctl", &["--user", "daemon-reload"])?;
    } else {
        bail!("지원하지 않는 OS");
    }
    println!("해제 완료");
    Ok(())
}

pub fn restart() -> anyhow::Result<()> {
    let ok = if cfg!(target_os = "macos") {
        let uid = current_uid()?;
        run_cmd("launchctl", &["kickstart", "-k", &format!("gui/{uid}/{SERVICE_LABEL}")])?
    } else if cfg!(target_os = "linux") {
        run_cmd("systemctl", &["--user", "restart", "wimcc"])?
    } else {
        bail!("지원하지 않는 OS")
    };
    println!("{}", if ok { "재시작 완료" } else { "재시작 실패 — status로 확인" });
    Ok(())
}

pub fn status() -> anyhow::Result<()> {
    let ok = if cfg!(target_os = "macos") {
        let uid = current_uid()?;
        run_cmd("launchctl", &["print", &format!("gui/{uid}/{SERVICE_LABEL}")])?
    } else if cfg!(target_os = "linux") {
        run_cmd("systemctl", &["--user", "is-active", "wimcc"])?
    } else {
        bail!("지원하지 않는 OS")
    };
    println!("{}", if ok { "등록됨/실행 중" } else { "미등록 또는 정지" });
    Ok(())
}
```

`src/cli.rs`:

```rust
    /// serve를 OS 사용자 서비스로 등록/해제한다 (macOS launchd, Linux systemd --user).
    Service {
        #[command(subcommand)]
        action: ServiceAction,
    },
```

```rust
#[derive(Debug, Subcommand)]
pub enum ServiceAction {
    /// 로그인 시 자동 시작되도록 등록. --db-path(전역)는 절대경로로 기록된다.
    Install {
        #[arg(long, default_value = "127.0.0.1")]
        bind: std::net::IpAddr,
        #[arg(long, default_value_t = 7878)]
        port: u16,
        /// 서비스는 무인 기동이라 마이그레이션 프롬프트에 답할 수 없다 — 기본 on.
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        auto_migrate: bool,
    },
    Uninstall,
    Restart,
    Status,
}
```

`src/cli.rs` 테스트 모듈에:

```rust
    #[test]
    fn service_install_defaults() {
        let cli = Cli::try_parse_from(["wimcc", "service", "install"]).expect("parses");
        match cli.command {
            Command::Service { action: ServiceAction::Install { port, auto_migrate, .. } } => {
                assert_eq!(port, 7878);
                assert!(auto_migrate, "무인 기동이므로 auto-migrate 기본 on");
            }
            other => panic!("expected Service Install, got {other:?}"),
        }
    }
```

`src/main.rs` arm (기존 패턴대로 — `cli.db_path` 전역 인자를 install에 전달):

```rust
        Command::Service { action } => match action {
            ServiceAction::Install { bind, port, auto_migrate } => {
                wimcc::service::install(&cli.db_path, &bind.to_string(), port, auto_migrate)?
            }
            ServiceAction::Uninstall => wimcc::service::uninstall()?,
            ServiceAction::Restart => wimcc::service::restart()?,
            ServiceAction::Status => wimcc::service::status()?,
        },
```

- [ ] **Step 4: 스냅샷 수락 + green 확인**

Run: `cargo test --lib service 2>&1` → 스냅샷 신규 생성으로 실패 → `cargo insta accept` → 재실행.
Expected: 스냅샷 2건 수락 후 passed. 스냅샷 내용 육안 확인: plist에 argv 순서 그대로, unit ExecStart에 인용 처리.

- [ ] **Step 5: 로컬 스모크 (스크래치 — 운영 :7878 금지)**

```bash
cargo build
./target/debug/wimcc --db-path /tmp/wimcc-svc-smoke.sqlite service install --port 7999
./target/debug/wimcc service status          # 등록됨/실행 중
curl -s http://127.0.0.1:7999/v1/health | head -c 120   # serve 응답 확인
./target/debug/wimcc service restart
./target/debug/wimcc service uninstall
./target/debug/wimcc service status          # 미등록 또는 정지
```

Expected: 각 단계 표기대로. 스모크 후 `/tmp/wimcc-svc-smoke.sqlite` 삭제.

- [ ] **Step 6: 커밋**

```bash
git add src/service.rs src/lib.rs src/cli.rs src/main.rs src/snapshots/
git commit -m "feat(cli): wimcc service — launchd/systemd user 서비스 등록·해제·재시작·상태"
```

(insta 스냅샷 저장 경로가 `src/snapshots/`가 아니면 실제 생성 경로를 add.)

---

### Task 8: 문서 — README 설치 섹션 + implementation-notes + CLAUDE.md

**Files:**
- Modify: `README.md`, `README.ko.md`, `docs/implementation-notes.html`, `docs/notes-index.md`, `CLAUDE.md`(Operations 한 줄)

- [ ] **Step 1: README.md에 Install/Update 섹션**

기존 개발용 빌드 안내 위/앞에 추가:

````markdown
## Install

```sh
# shell (macOS / Linux)
curl -fsSL https://github.com/bahamoth/whats-in-my-cc/releases/latest/download/wimcc-installer.sh | sh

# Homebrew
brew install bahamoth/tap/wimcc

# npm (Claude Code users already have this)
npm install -g wimcc

# cargo
cargo install wimcc          # or: cargo binstall wimcc

# mise
mise use -g ubi:bahamoth/whats-in-my-cc
```

## Update

- shell install: `wimcc self-update` (check only: `wimcc self-update --check`)
- brew / npm / cargo: use that manager's upgrade command
- The running `wimcc serve` keeps the old binary until restarted — restart when
  no live Claude Code session is being observed: `wimcc service restart`
- `wimcc serve` checks GitHub Releases metadata once a day (its only outbound
  call) and shows a banner in the WebUI; disable with `--update-check off`
  or `WIMCC_UPDATE_CHECK=off`

## Run as a service

```sh
wimcc service install    # start on login (launchd / systemd --user)
wimcc service status
wimcc service restart    # e.g. after self-update
wimcc service uninstall
```
````

installer 스크립트 실제 파일명은 Task 2 Step 5의 `dist plan` 실측(`wimcc-installer.sh` 예상)과 일치시킨다.

- [ ] **Step 2: README.ko.md에 동일 섹션 한국어판**

같은 코드 블록 + 한국어 설명(Update 절: "실행 중인 serve는 재시작 전까지 구 바이너리로 동작 — 라이브 Claude Code 세션 관측이 없을 때 `wimcc service restart`").

- [ ] **Step 3: implementation-notes + notes-index**

`docs/implementation-notes.html`에 앵커 `#release-distribution` 섹션 추가 — 내용(요지):

- dist·release-please 연동: release-please가 태그+Release 생성, dist `create-release = false`로 업로드만. 근거와 함정(PAT 필요 — `github.token` 태그는 워크플로 미발화).
- crates.io `include` 허용목록 전환 함정: 명시 파일만 패키징 — 신규 임베드 자산이 생기면 include·게이트 스크립트에 추가해야 한다.
- self-update receipt 분기: axoupdater는 shell installer receipt 기반 — 패키지 매니저 설치본은 안내만(의도된 동작).
- 첫 릴리스 검증 체크리스트(스펙 §8) 전사.

`docs/notes-index.md`에 한 줄 추가 (기존 형식대로): `release-distribution` → 위 앵커.

- [ ] **Step 4: CLAUDE.md Operations 정정**

"CI/릴리스" 불릿의 "바이너리 업로드" 서술을 현행화: 릴리스 산출(4타깃 빌드·installer·brew/npm/crates.io 퍼블리시)은 dist 워크플로(`release.yml`) 소관, release-please는 버전·태그·CHANGELOG.

- [ ] **Step 5: 커밋**

```bash
git add README.md README.ko.md docs/implementation-notes.html docs/notes-index.md CLAUDE.md
git commit -m "docs: 설치·업데이트·서비스 안내 + release-distribution 구현 노트"
```

---

### Task 9: 개선 루프 + 전체 검증 + PR

- [ ] **Step 1: 재빌드 + 개선 루프 4종 (PR 전 필수)**

```bash
cargo build --release
cd webui && node scripts/untagged-bash.ts --all
cd webui && node scripts/unknown-verification.ts --all
cd webui && node scripts/unidentified-plugins.ts --all
cd webui && node scripts/tagging-gate.ts
```

Expected: tagging-gate exit 0. 보편 후보가 나오면 사전(`src/insight/event_tags.rs` 등)에 TDD로 추가, 비보편은 `scripts/tagging-gate-baseline.json`에 사유와 함께 보류 커밋.

- [ ] **Step 2: 전체 검증 (무거운 것은 백그라운드, 출력 위생 준수)**

```bash
cargo fmt; echo EXIT=$?
cargo clippy --all-targets -- -D warnings   # run_in_background
cargo test                                   # run_in_background
cd webui && npx vitest run 2>&1
bash scripts/check-crate-contents.sh
```

Expected: fmt EXIT=0, clippy 경고 0, cargo test 전부 passed(요약 줄 원문 확인 — "0 failed"), vitest 전부 passed, 게이트 OK.

- [ ] **Step 3: Self-check before commit 목록 점검**

CLAUDE.md의 6항목 확인 — 특히 ① 각 변경에 테스트 존재 ② 외부 주장(axoupdater receipt·GitHub API)이 fixture/공식 문서로 잠겼는지 ③ UI 브라우저 스모크 완료 여부.

- [ ] **Step 4: PR 생성 (self-merge 금지)**

```bash
git push -u origin feat/release-distribution
gh pr create --title "feat: 배포·릴리즈·업데이트 체계 — dist 4채널 + self-update + service" --body "$(cat <<'EOF'
스펙: docs/specs/2026-07-17-release-distribution.md / 계획: docs/plans/2026-07-18-release-distribution-plan.md

- dist(구 cargo-dist) 도입: 4타깃(mac aarch64/x86_64, linux musl x86_64/aarch64) 빌드, shell/brew/npm installer, crates.io publish — release-please는 태그·CHANGELOG·Release 생성 전담(create-release=false)
- wimcc self-update(axoupdater, receipt 분기 — 패키지 매니저 설치본은 안내만), 자동 재시작 없음
- serve 업데이트 체크(기본 on, --update-check off) + /v1/health version 블록 + WebUI 배너
- wimcc service install/uninstall/restart/status (launchd/systemd user)
- LICENSE(MIT OR Apache-2.0), crates.io include 허용목록 + 동봉 게이트

### 머지 전 필요 secrets
NPM_TOKEN · CARGO_REGISTRY_TOKEN · HOMEBREW_TAP_TOKEN(+ bahamoth/homebrew-tap repo) · RELEASE_PLEASE PAT

### 첫 릴리스에서 검증 (스펙 §8)
- [ ] 태그 push가 release.yml을 트리거하는가 (PAT)
- [ ] 4타깃 asset + install.sh + sha256 업로드
- [ ] 기존 Release에 업로드됐는가 (create-release=false 동작)
- [ ] brew formula / npm / crates.io 퍼블리시 성공
- [ ] 각 채널 설치 스모크 + wimcc self-update 동작
EOF
)"
```

CI green 확인까지가 이 계획의 종료 조건. 머지는 사용자가 rebase로 수행한다.

---

## 첫 릴리스 후속 (이 PR 범위 밖 — 머지 후 다음 릴리스에서)

PR body의 체크리스트를 그 시점에 검증하고, 실패 항목은 implementation-notes `#release-distribution`에 결과와 함께 기록한다. `create-release = false`가 published Release와 충돌하면 폴백: release-please `skip-github-release: true` + dist `create-release = true`(CHANGELOG 본문은 dist가 태그 기준 추출).
