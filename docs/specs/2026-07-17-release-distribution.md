# 배포·릴리즈·업데이트 체계 — 확정 스펙 (2026-07-17)

현행(소스 빌드가 유일한 설치 경로, 업데이트 수단 없음)을 개선해 설치 채널·self-update·
업데이트 알림·서비스 등록까지 **한 번에** 갖춘다(단계적 도입 대신 일괄 구성 — 사용자 결정).
브레인스토밍 대화(2026-07-17)에서 모든 축을 사용자와 확정했다.

## 0. 확정 결정

| 축 | 결정 |
|----|------|
| 릴리스 도구 | **dist**(구 cargo-dist, astral 유지 fork)로 빌드·installer·퍼블리시 일원화. release-please는 현행 유지 |
| 설치 채널 | shell installer(`curl \| sh`) · Homebrew tap · npm(`wimcc`, unscoped) · crates.io(+cargo-binstall) · mise/ubi(문서만) |
| 빌드 타깃 | `aarch64-apple-darwin` · `x86_64-apple-darwin` · `x86_64-unknown-linux-musl` · `aarch64-unknown-linux-musl` (Windows 제외 — 동작 미검증) |
| self-update | `wimcc self-update` 서브커맨드(axoupdater 라이브러리 통합) |
| 업데이트 체크 | **기본 켬** + config로 완전 비활성(opt-out). serve가 주기 조회 → WebUI 배너 + CLI 로그 |
| crates.io | **이번에 포함**(기존 "추후 결정" 보류를 사용자가 해제). `webui/dist` 동봉 + binstall 메타데이터 |
| 서비스 등록 | `wimcc service install/uninstall/restart/status` 포함(macOS launchd + Linux systemd user unit) |

npm 이름 `wimcc`·crates.io 이름 `wimcc`는 2026-07-17 조회로 비어 있음 확인(npm 404,
crates.io "does not exist").

## 1. 파이프라인 아키텍처

역할 분담 — **release-please는 버전 산정·릴리스 PR·태그·CHANGELOG까지**(현행 유지,
버전 파일 수동 수정 금지 원칙 동일), **dist가 태그 이후를 전담**:

```
conventional commits → release-please PR → 머지 → vX.Y.Z 태그
  → dist 워크플로 (태그 트리거)
      ├─ 4타깃 빌드 (github-build-setup으로 webui npm build 선행 → rust-embed 임베드)
      ├─ shell installer(install.sh) + sha256 체크섬 생성·업로드
      ├─ Homebrew formula → bahamoth/homebrew-tap 퍼블리시 (publish-jobs)
      ├─ npm 패키지 `wimcc` 퍼블리시 (publish-jobs)
      └─ cargo publish (crates.io)
```

- 기존 수제 `upload-release-binaries` job은 **제거**(dist가 대체).
- **webui 임베드**: dist 설정 `github-build-setup`으로 `build-local-artifacts` job의
  cargo 빌드 앞에 `setup-node` + `npm ci` + `npm run build` 스텝을 주입한다
  (dist 공식 문서의 비-Rust 빌드 의존성 메커니즘 — Tauri 사례로 안내됨).
- **GitHub Release 생성 주체 조정(열린 검증 항목)**: release-please와 dist 모두
  Release를 만들 수 있어 충돌 — 구현 시 한쪽을 끄는 설정으로 해소한다. 어느 쪽이
  만들든 CHANGELOG 본문은 유지.
- **musl 정적 빌드 실현 가능성**: OpenSSL 의존 없음(reqwest `rustls-tls`,
  sqlx sqlite 번들) — 2026-07-17 `Cargo.toml` 실측. cross 빌드 러너
  (GitHub ARM 러너 vs dist cross 지원)는 구현 시 결정.

## 2. 설치 채널 (사용자 관점 계약)

| 채널 | 설치 | 업데이트 |
|------|------|---------|
| shell | `curl -fsSL …/install.sh \| sh` | `wimcc self-update` |
| Homebrew | `brew install bahamoth/tap/wimcc` | `brew upgrade wimcc` |
| npm | `npm i -g wimcc` | `npm update -g wimcc` |
| cargo | `cargo install wimcc` / `cargo binstall wimcc` | 동일 명령 재실행 |
| mise | `mise use -g ubi:bahamoth/whats-in-my-cc` | `mise upgrade` |

- npm 채널의 근거: 타깃 사용자(Claude Code 사용자)는 전원 npm 보유.
- mise/ubi는 릴리스 asset 네이밍이 규칙적이면 추가 작업 없음 — README 한 줄만.

### crates.io 동봉 규칙

`cargo install`은 사용자 머신 소스 빌드이므로 `.crate`에 webui 빌드 산출물이
있어야 한다(rust-embed는 컴파일 타임 임베드):

- `Cargo.toml` `package.include`에 `webui/dist/**` 추가.
- CI에 **packaging 검증 스텝**: `cargo package --list` 출력에 `webui/dist` 포함을
  assert — 빠지면 사용자 쪽 컴파일 실패/빈 UI가 되므로 게이트로 잠근다.
- `[package.metadata.binstall]`: GitHub Releases 바이너리를 재사용하는 pkg-url
  템플릿 — binstall 사용자는 재컴파일하지 않는다.

## 3. `wimcc self-update`

axoupdater를 라이브러리로 통합한 서브커맨드. axoupdater는 shell installer가 남기는
**install receipt 기반**이므로 설치 경로별 분기가 계약이다:

| 설치 경로 | `wimcc self-update` 동작 |
|-----------|------------------------|
| shell installer (receipt 있음) | 최신 확인 → 다운로드 → 바이너리 원자적 교체 |
| brew/npm/cargo (receipt 없음) | 해당 매니저 명령 안내 후 종료(예: "brew upgrade wimcc 사용") — 매니저 관리 파일을 임의 교체하면 매니저 상태가 깨지므로 의도된 동작 |
| `--check` 플래그 | 어느 경로든 조회만, 교체 없음 |

**자동 재시작 금지(불변)**: 어떤 경우에도 실행 중 serve를 재시작하지 않는다 —
serve 재기동은 라이브 CC 세션 관측을 중단시킬 수 있다. 교체 성공 시 "라이브 세션이
없을 때 `wimcc service restart`(또는 수동 재기동)" 안내만 출력한다.

## 4. 업데이트 체크 + WebUI 배너

- serve가 **시작 시 + 24시간 주기**로 GitHub Releases 최신 버전 메타데이터만
  조회·캐시. 이것이 wimcc의 **유일한 outbound 호출**이다 — README·설정 문서에 명시.
- config 설정 한 줄로 완전 비활성(기본 켬 — 편의성 우선 사용자 결정. Local-first
  원칙은 inbound bind에 관한 것이므로 충돌 없음).
- 조회 결과는 기존 `/v1/health` 응답에 현재/최신 버전 필드로 실어 WebUI 배너 표시 —
  현재/최신 **버전 숫자 병기**(판정 문장 금지 원칙 준수), serve 시작 로그에도 한 줄.
  endpoint 계약 변경은 `docs/04_api_mcp_spec.html`에 같은 PR에서 반영.
- 조회 실패(오프라인 등)는 조용히 무시 — 기능 동작에 영향 없음. 재시도는 다음 주기.

## 5. `wimcc service` 서브커맨드

- `install` / `uninstall` / `restart` / `status`.
- macOS: `~/Library/LaunchAgents/` plist 생성 + `launchctl` 등록.
  Linux: `~/.config/systemd/user/` unit + `systemctl --user`.
- serve 인자(port·db-path·log-dir 등)는 install 시점 플래그로 받아 plist/unit에 기록.
- 실행 주체는 항상 최종 사용자 — 설치·해제 모두 명령으로 가역.
- 검증은 스크래치 환경(별도 포트·DB) 등록→상태→해제 스모크로 한정.
  **운영 serve(:7878)는 건드리지 않는다.**

## 6. 테스트 전략 (TDD red 우선)

| 대상 | 방법 |
|------|------|
| 버전 비교·업데이트 체크 | GitHub Releases API 실 응답 1회 채취 → `tests/fixtures/**/real/` 동결(Real-data anchoring), 단위 테스트. 테스트에서 실 네트워크 호출 없음 |
| self-update 분기 | receipt 유무별 안내 로직 단위 테스트 |
| service | plist/unit 생성 내용 insta 스냅샷 + 로컬 등록/해제 스모크 |
| packaging | `cargo package --list`에 `webui/dist` 포함 assert (CI 게이트) |
| WebUI 배너 | vitest + 브라우저 스모크 후 commit (UI 원칙) |
| 릴리스 파이프라인 | 태그가 있어야 최종 검증 가능 — 다음 실제 릴리스가 검증 이벤트. §8 체크리스트로 확인 |

## 7. 사전 준비물 (사용자 액션 필요)

1. npm 계정 + repo secret `NPM_TOKEN`
2. crates.io 계정 + repo secret `CARGO_REGISTRY_TOKEN`
3. `bahamoth/homebrew-tap` 저장소 생성 + 그 repo write 권한 PAT를 secret으로

## 8. 구현 중 검증 항목 (열린 것)

- [ ] GitHub Release 생성 주체 조정 — release-please vs dist 중 한쪽 생성 비활성 (§1)
- [ ] aarch64-linux 빌드 러너 — GitHub ARM 러너 vs dist cross 지원 (§1)
- [ ] axoupdater receipt 동작 실측 — 문서 + 로컬 실험 (§3)
- [ ] 첫 릴리스에서: 4타깃 asset·install.sh·sha256·brew formula·npm·crates.io 전부
  발행 확인 + 각 채널 설치 스모크

## Non-goals

- Windows 빌드·지원(동작 미검증 — 별도 검증 작업 후 재론)
- serve 자동 재시작을 포함한 무인 자동 업데이트
- 버전 메타데이터 조회를 넘는 어떤 telemetry·외부 write
