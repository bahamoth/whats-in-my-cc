# wimcc — What's in My Claude Code

[![CI](https://github.com/bahamoth/whats-in-my-cc/actions/workflows/ci.yml/badge.svg)](https://github.com/bahamoth/whats-in-my-cc/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/bahamoth/whats-in-my-cc)](https://github.com/bahamoth/whats-in-my-cc/releases/latest)

[English](README.md) · **한국어**

**Claude Code의 모든 내부 동작을 측정하고 기록해 사람과 에이전트 모두에게 실행 가시성을 제공합니다.**

## What's in my cc?

wimcc는 방금 끝났거나 지금 진행 중인 Claude Code 세션을, 한 단계씩 짚어볼 수 있는
실행으로 다시 보여준다. 채팅 로그만으로는 안 보이는 것들 — 어떤 도구가 왜 실패했는지,
모델 요청에 시간과 토큰이 얼마나 들었는지, hook이 무엇을 막았는지, 어떤 edit이 어느
파일의 어느 줄을 바꿨는지 — 을 한 화면에 모아 보여주고, 각 항목을 그 근거가 된 원본
기록으로 바로 되짚을 수 있게 한다.

wimcc로 할 수 있는 것:

- 이 세션에서 **어떤 tool call이 실패했고 왜** 그랬는지 본다.
- **토큰을 어디서 얼마나** 썼는지 — usage·비용·컨텍스트 효율을 추적한다.
- **어떤 edit이 어느 파일의 어느 라인**을 바꿨는지 되짚는다 (파일 lineage).
- **hook이 무엇을 막거나 통과**시켰는지 확인한다.
- 모델 요청·도구 호출·hook이 **시간순으로 어떻게 얽혀** 흘렀는지 따라간다.
- 위 모든 것을 브라우저 UI로 보거나, **Pull API / MCP로 다른 도구·에이전트가** 가져간다.

모든 처리는 로컬(`127.0.0.1`)에서 이루어지며 외부로 데이터를 전송하지 않는다. 외부
접근은 읽기 전용이다.

## 빠른 시작

### 빌드된 바이너리

[GitHub Releases](https://github.com/bahamoth/whats-in-my-cc/releases/latest)에서
플랫폼에 맞는 아카이브를 받는다 — Linux `x86_64` 또는 macOS Apple Silicon.
WebUI가 임베드되어 있어 단일 바이너리만 있으면 된다.

```bash
tar -xzf wimcc-v*-aarch64-apple-darwin.tar.gz   # 또는 …-x86_64-unknown-linux-gnu.tar.gz
./wimcc init-db                # 마이그레이션 적용, .wimcc.sqlite 준비
./wimcc serve --auto-migrate   # http://127.0.0.1:7878  (auth 기본 off)
./wimcc doctor                 # collector 연동 점검
```

### 소스에서 빌드

```bash
just build-release                            # WebUI 빌드 + 릴리스 바이너리(target/release/wimcc)를 한 번에
./target/release/wimcc init-db                # 마이그레이션 적용, .wimcc.sqlite 준비
./target/release/wimcc serve --auto-migrate   # http://127.0.0.1:7878  (auth 기본 off)
./target/release/wimcc doctor                 # collector 연동 점검
```

`serve`는 한 프로세스에서 전부 실행한다: read-only Pull API, 임베디드 WebUI, OTel
수신기, 그리고 `~/.claude/projects` transcript live tail. Claude Code가 OTel
event를 wimcc로 내보내게 하려면 `~/.claude/settings.json`을 한 번 연동한다 —
[Claude Code 연동](#claude-code-연동) 참고. `wimcc doctor`가 무엇이 연동됐고 무엇이
빠졌는지 알려준다.

`wimcc ingest --all`은 백필(기존 transcript JSONL을 cold-start로 한 번 훑기)용으로
여전히 쓸 수 있지만, live 운용에는 필요하지 않다.

### 개발 환경

핫 리로드로 개발할 때는 `just dev`가 백엔드와 vite dev 서버를 함께 띄운다:

```bash
just dev   # 백엔드 :7878 + vite :5173 (HMR); Ctrl-C로 둘 다 종료
```

브라우저는 `http://127.0.0.1:5173`. 테스트·빌드 등 전체 recipe는 아래 **빌드 · 테스트 · 개발** 절 참고.

## CLI

```
wimcc [--db-path <PATH>] [--log-format pretty|json] [--verbose] <command>
```

전역 옵션은 모든 서브커맨드에 적용된다. `--db-path` 기본값은 `.wimcc.sqlite`
(env `WIMCC_DB`).

| 커맨드 | 역할 |
| --- | --- |
| `init-db` | 마이그레이션 적용 및 DB 준비. |
| `ingest --all` / `ingest --path <P>` | 백필: transcript JSONL을 raw + observed event로 스캔(idempotent). |
| `doctor [--json] [--server <URL>] [--project <DIR>]` | collector 연동 상태의 read-only 진단(설정 계층, OTel env, 서버 probe). 어떤 것도 변경하지 않음. |
| `serve` | 로컬 서비스 시작: Pull API + WebUI + OTel 수신기 + transcript live tail. |

### `wimcc serve` 옵션

```
wimcc serve [--bind 127.0.0.1] [--port 7878]
             [--auto-migrate]                    # 시작 시 대기 중 마이그레이션 적용
             [--transcripts-root <PATH>]         # ~/.claude/projects 재정의
             [--no-watch-transcripts]            # live tail 비활성화 (OTel/hook만)
             [--auth off|on]                      # /v1 + /mcp에 bearer-token 인증 (기본: off)
             [--retention-profile none|default|strict]   # 백그라운드 retention sweep (기본: none)
             [--print-token] [--rotate-token]     # bearer token 관리 후 종료
             [--sse-keepalive-secs N]             # WebUI live-stream keep-alive (기본: 30)
             [--sse-channel-capacity N]           # broadcast 채널 용량 (기본: 512)
             [--shutdown-after-ms N]              # test/smoke 편의
```

## 관측 대상

| 소스 | 도달 방식 | 비고 |
| --- | --- | --- |
| **Transcript** | `~/.claude/projects/**/*.jsonl` live tail (또는 `ingest` 백필) | user/assistant 메시지, tool call + result, thinking, attachment, hook 실행 결과 |
| **OTel traces / metrics / logs** | Claude Code OTLP exporter의 `POST /otel/v1/*` | traces는 beta (`CLAUDE_CODE_ENHANCED_TELEMETRY_BETA=1`) |
| **Edit diffs** | 각 transcript tool-result의 `toolUseResult.structuredPatch`에서 추출 | `Edit`만 hunk 생성, `Write`는 빈 patch. `/v1/sessions/:id/diff-hunks`와 `get_file_lineage` MCP tool을 구동 |
| **Verification run / 토큰 usage** | transcript + telemetry facet에서 도출 | verification-run·usage 엔드포인트로 노출 |

## 엔드포인트

대부분의 `GET /v1/*` 응답은 `{meta: {schema_version, collection_profile,
redaction_policy, …}, data}`로 감싼다 — 예외: `/v1/health`는 bare JSON,
metrics·signals·detectors·audit 엔드포인트는 `{data}`만, MCP tool 결과는
JSON-RPC content다. `--auth on`이면 모든 `/v1/*`·`/mcp` 요청에
`Authorization: Bearer <token>`이 필요하다. OTel collector와 SSE 스트림은 항상
인증 없는 loopback 엔드포인트다.

### Read-only Pull API (`GET`)

| 경로 | 응답 |
| --- | --- |
| `/v1/health` | `{status, build_sha, security: {auth_required, retention_profile}}` |
| `/v1/health/sources` | 소스별 freshness (`doctor`가 사용) |
| `/v1/sessions` | 세션 목록 (최신순) |
| `/v1/sessions/{id}` | `{session_id, summary}` (event는 `/v1/sessions/{id}/events`로 조회) |
| `/v1/sessions/{id}/events` | 페이지된 observed event |
| `/v1/sessions/{id}/diff-hunks` | 세션의 edit hunk |
| `/v1/sessions/{id}/usage` | 토큰 usage 롤업 (`assistant_events`, `user_turns`, 토큰, 추정 비용) |
| `/v1/sessions/{id}/metrics` | 온디맨드 세션 행동 지표 — 합성 가능한 count만 (rate 없음) |
| `/v1/sessions/{id}/signals` | 결정론 detector signal (evidence-linked) |
| `/v1/signals/{id}` | 단일 signal |
| `/v1/sessions/{id}/verification-runs` | 세션 내 verification run |
| `/v1/verification-runs/{id}` | 단일 verification run |
| `/v1/usage/baseline` | 세션 간 usage baseline (p25/median/p75) |
| `/v1/detectors` | detector manifest (deterministic L1 detector 5종) |
| `/v1/events/{event_id}/raw` | 한 event의 source-preserving raw payload |
| `/v1/audit` | audit 로그 |
| `/v1/stream` | Server-Sent Events live 스트림 (WebUI 구동) |

### Collector (`POST`, loopback, 인증 없음)

| 경로 | 신호 |
| --- | --- |
| `/otel/v1/traces` | OTel traces (beta) |
| `/otel/v1/metrics` | OTel metrics |
| `/otel/v1/logs` | OTel logs |

OTLP body는 OTLP/JSON, gzip 선택, ≤ 4 MB. `/otel` prefix는 **필수**다 — 없으면 OTel
SDK가 `…/v1/metrics`로 POST해서 wimcc가 404를 반환한다.

### MCP (Streamable HTTP)

`POST`/`GET /mcp`는 동일한 read-only 데이터를 MCP tool로 노출한다:

- `whats_in_my_cc.search_sessions`
- `whats_in_my_cc.get_file_lineage`
- `whats_in_my_cc.get_otel_trace`
- `whats_in_my_cc.list_detectors`

MCP resource도 제공한다: 세션별 summary, 그리고 file-lineage·OTel-trace
resource template.

## Web UI

`wimcc` 바이너리는 React SPA를 임베드(rust-embed)하며 `http://127.0.0.1:7878/`에서
제공한다. 두 페이지:

- `/sessions` — 세션 목록
- `/sessions/:id` — event-first replay: event별 detail panel을 갖춘 conversation
  stream, raw-source 탭, insight strip(컨텍스트 효율/토큰/검증/도구 실패/비용,
  provenance 배지), 분석 패널(세션 지표 + detector 발화 분포), 그리고 tagging loop용
  untagged-Bash 패널.

로컬 개발(dev 서버·빌드·테스트)은 아래 **빌드 · 테스트 · 개발** 섹션을 참고한다.

## Claude Code 연동

wimcc는 `settings.json`을 자동으로 수정하지 않는다 — 아래 블록을 직접 한 번 추가한 뒤
`wimcc doctor`로 scope 귀속을 확인한다.

### OTel

```jsonc
{
  "env": {
    "CLAUDE_CODE_ENABLE_TELEMETRY": "1",
    "CLAUDE_CODE_ENHANCED_TELEMETRY_BETA": "1",
    "OTEL_METRICS_EXPORTER": "otlp",
    "OTEL_LOGS_EXPORTER":    "otlp",
    "OTEL_TRACES_EXPORTER":  "otlp",
    "OTEL_EXPORTER_OTLP_PROTOCOL": "http/json",
    "OTEL_EXPORTER_OTLP_ENDPOINT": "http://localhost:7878/otel",
    "OTEL_METRIC_EXPORT_INTERVAL": "5000",
    "OTEL_LOGS_EXPORT_INTERVAL":   "2000",
    "OTEL_TRACES_EXPORT_INTERVAL": "2000"
  }
}
```

traces는 beta다 — `CLAUDE_CODE_ENHANCED_TELEMETRY_BETA=1`이 없으면 SDK가 span을
전혀 내보내지 않는다. `session.id`가 없는 레코드는 저장되지만 `/v1/sessions`에서는
제외된다.

> Hook lifecycle event는 transcript live tail에서 수집된다(hook 실행 결과가
> transcript에 남는다). 따라서 forward 스크립트 연동이 필요 없다. `/hooks/v1/events`
> collector는 2026-06에 제거됐다 — `docs/implementation-notes.html` 참고. 기본(`7878`)과
> 다른 포트로 띄우면 `WIMCC_PORT`로 지정 — `session-retrospect` 플러그인의 MCP 연결이
> 따라간다(예: `WIMCC_PORT=9000`).

### Smoke test

```bash
curl -X POST http://127.0.0.1:7878/otel/v1/metrics \
  -H 'Content-Type: application/json' \
  --data-binary @tests/fixtures/otel/real/metrics_v01.json
```

## 인증 & retention

- **인증**은 기본 `off`(단일 사용자 로컬 dev) — 브라우저로 바로 접속. `wimcc serve
  --auth on`은 `/v1/*` + `/mcp`에 bearer token을 강제한다. Token 파일: macOS
  `~/Library/Application Support/wimcc/token`, Linux `~/.config/wimcc/token`
  (mode `0600`). `serve --print-token` / `--rotate-token`으로 관리한다.
- **retention**은 기본 `none`(삭제 없음). `--retention-profile default`
  (raw 30d / normalized 180d / insight 180d / audit 90d) 또는 `strict`
  (raw 7d / normalized 30d / insight 30d / audit 30d)로 백그라운드 sweep을
  활성화한다.

## 보안 주의

- **ingest 시점 redaction**이 raw payload 저장 전에 알려진 secret 패턴을 마스킹하고
  (rule pack v1) event별 `redaction_manifest`를 남긴다. high-entropy 문자열은
  flag만 하며 export-side review는 없다 — SQLite 파일과 `127.0.0.1`로 닿는 모든 것은
  여전히 민감하게 취급할 것.
- diff hunk는 transcript `structuredPatch` 텍스트에서만 생성되며, 긴 patch
  preview는 일정 크기로 truncate된다.
- OTel real-fixture freeze 스크립트는 PII를 안정적 placeholder로 자동 redact하지만,
  fixture를 커밋하기 전 항상 본인 이메일을 grep할 것.

## 빌드 · 테스트 · 개발

빌드·테스트는 `just` 레시피로 묶여 있다. 백엔드 바이너리는 `webui/dist/`를 rust-embed로
컴파일 시점에 임베드하므로, **백엔드를 빌드·테스트하기 전에 SPA가 먼저 빌드돼 있어야
한다.**

| 레시피 | 동작 |
| --- | --- |
| `just webui-install` | webui npm 의존성 설치 (idempotent) |
| `just webui-build` | SPA 프로덕션 빌드 → `webui/dist/` (`tsc -b && vite build`) |
| `just webui-test` | 프론트엔드 단위 테스트 (`vitest run`) |
| `just webui-dev` | vite dev 서버 (`127.0.0.1:5173`, `/v1` → `7878` 프록시) |
| `just serve-dev` | 백엔드 dev 실행 (`cargo run -- serve --auto-migrate`) |
| `just dev` | dev 환경 일괄 구동 — 백엔드 + vite(HMR) 동시; Ctrl-C로 둘 다 종료 |
| `just build-release` | `webui-build` 후 `cargo build --release` → `target/release/wimcc` |

**릴리스 빌드** — WebUI가 임베드된 단일 바이너리:

```
just build-release
./target/release/wimcc serve --auto-migrate
```

**백엔드 테스트** — `cargo test`. rust-embed가 컴파일 시점에 `webui/dist/`를 요구하므로,
새로 clone했다면 SPA를 먼저 한 번 빌드한다:

```
just webui-build
cargo test
```

**프론트엔드 테스트** — `just webui-test` (`vitest run`). watch 모드는
`cd webui && npm run test:watch`.

**개발 환경** — `just dev` 한 번으로 백엔드(`:7878`)와 vite dev 서버(`:5173`, HMR)를
함께 띄운다; Ctrl-C로 둘 다 종료된다. 브라우저는 `http://127.0.0.1:5173`(vite가 `/v1`을
— `/v1/stream` SSE 포함 — 백엔드로 프록시; `WIMCC_PROXY_TARGET`로 다른 serve 인스턴스 지정 가능).
따로 띄우려면 `just serve-dev`(백엔드)와 `just webui-dev`(프론트엔드).

**Node 버전** — 빌드는 Node 20 (`webui/.nvmrc`). untagged-Bash 도구 스크립트
(`webui/scripts/untagged-bash.ts`)는 네이티브 타입 스트리핑을 위해 Node 22+가 필요하다.

**dev DB 재생성** — 마이그레이션 변경(현재 head는 `migrations/` 참조) 후에는 `wimcc init-db` + 재ingest가
필요하다. JSON BLOB으로 저장되는 payload 필드(`tool_call.tool_name`,
`assistant_message.model` 등)는 schema migration 없이 추가되므로, 기존 event는
재ingest해야 채워진다.

## CI & 릴리스

- **CI** (GitHub Actions)는 모든 PR에서 전체 게이트를 돌린다: `vitest` + SPA
  빌드, 이어서 갓 빌드된 `webui/dist`를 대상으로 `cargo fmt --check`,
  `cargo clippy -- -D warnings`, `cargo test`.
- **릴리스**는 release-please로 자동화되어 있다: `main`의 conventional commit이
  릴리스 PR에 누적되고, 그 PR을 머지하면 `vX.Y.Z` 태그·CHANGELOG 생성과 함께
  `wimcc` 바이너리(Linux `x86_64`, macOS Apple Silicon)가 GitHub Release에
  업로드된다. `Cargo.toml`과 `webui/package.json`의 버전은 함께 bump되므로
  손으로 수정하지 말 것.

## 참고 문서

- 전체 시스템 사양: `docs/index.html`, `docs/00..05_*.html`
- 구현 노트(편차·결정·event-first 재설계): `docs/implementation-notes.html`
- 기여자용 프로젝트 가이드: `CLAUDE.md`

