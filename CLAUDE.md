# CLAUDE.md — What's in My Claude Code

## Project Overview

Claude Code 실행을 **로컬에서** 관측해 **event-first 실행 기록**(`ObservedEvent` +
correlation 키)과 **evidence-linked Signal**을 만드는 로컬 서비스. 사용자는 chat
transcript가 아닌 **execution replay**로 "무엇이·왜 그렇게 됐는가"를 본다.

- 산출물: 개선 patch가 아니라 **deterministic L1 Signal + 온디맨드 SessionMetrics**
- 외부 접근: Pull API / MCP Streamable HTTP — **read-only**, 기본 bind `127.0.0.1`
- 설계 이력 SSOT: git history + `docs/implementation-notes.html`

## Document Map

작업 시작 전 관련 사양서를 먼저 읽는다(모두 자기완결 HTML — 외부 JS/CSS 없음).

| 작업 영역 | 먼저 읽을 문서 |
|----------|--------------|
| 제품 요구·범위·non-goals | `docs/00_prd_revised.html` |
| UX·screen·replay·insight strip·분석 패널 | `docs/01_product_design_spec.html` |
| 파이프라인·store·OTel 연동 | `docs/02_technical_architecture_spec.html` |
| 스키마 (ObservedEvent, Signal, VerificationRun, Metrics…) | `docs/03_data_model_spec.html` |
| HTTP / MCP endpoint 계약 | `docs/04_api_mcp_spec.html` |
| Collection profile·redaction·export | `docs/05_security_governance_spec.html` |

`docs/index.html`은 포털.

## Working Principles (반드시 지킬 것)

데이터 모델 설계 원칙(OTel-first·source-preserving·evidence-linked·no-annotation-model·
schema-versioning·deterministic-L1)의 SSOT는 `docs/03_data_model_spec.html` §01이다 —
여기서 재진술하지 않는다(중복은 곧 드리프트). 스키마/데이터 모델 작업 전 그 문서를 읽는다.

- **측정과 판별 분리**: 측정·집계·결정론 detector는 wimcc가 한다. "교정성 메시지" 류
  *의미 판별*은 lexical 휴리스틱으로 detector에 넣지 말고 LLM 온디맨드(retrospect)에 맡긴다.
- **Local-first**: 기본 bind `127.0.0.1`. `0.0.0.0`은 explicit setting일 때만.
- **TDD red 우선**: 어떤 변경이든 *실패하는* 테스트를 먼저 작성해 빨강을 확인한 뒤 구현한다.
  test가 후행이거나 omit된 commit은 만들지 않는다(단순 doc 변경 예외).
- **Real-data anchoring**: 외부 source(transcript JSONL·OTLP·hook stdin·git2 등)의 attribute
  의미 주장은 (a) 공식 docs 인용, 또는 (b) `tests/fixtures/**/real/`에 동결한 실 payload의
  invariant assertion으로 잠근다. 둘 다 없는 가정은 적지 않는다. 표본 1건으로 일반화하지 않는다.
- **UI는 브라우저 smoke 후 commit**: WebUI 변경은 `cargo build`+`vitest` 통과만으론 미완.
  `wimcc serve` + 브라우저 navigation·시각 검증까지 끝낸 뒤 commit.

## Operations

- **CI/릴리스**: 모든 PR에서 GitHub Actions CI(vitest+SPA build 후 그 dist로
  `cargo fmt --check`·`clippy -- -D warnings`·`cargo test`). 릴리스는 release-please —
  conventional commit이 릴리스 PR로 누적되고 머지 시 `vX.Y.Z` 태그·CHANGELOG·바이너리
  업로드. 버전(`Cargo.toml`·`webui/package.json`·`package-lock.json`)은 자동 bump —
  **손으로 수정 금지**. **PR 병합은 rebase**(merge commit 없는 linear history, squash 금지) —
  개별 conventional commit을 보존해 release-please가 type별로 버전을 산정한다.
- **인증**: 기본 `--auth off`(단일 사용자 dev). `--auth on`이면 `/v1/*`+`/mcp`에
  `Authorization: Bearer <token>` 필요(`/v1/stream`·collectors·SPA는 예외).
  token은 `dirs::config_dir()/wimcc/token`(0600).
- **dev DB**: migration 추가 시 원칙적으로 `wimcc init-db` + 재ingest. 단 serve/ingest
  startup에서 backfill하는 migration은 불필요(각 migration 주석이 명시). payload 필드
  (JSON BLOB)는 migration 없이 추가되므로 기존 이벤트엔 없다 — 재ingest해야 채워진다.

## Non-goals (절대 만들지 말 것)

- Claude Code 설정 / hook / command / skill / memory 변경
- 개선 patch / 설치 스크립트 자동 생성
- 외부 service-to-service correction·label·status write
- hidden reasoning / private chain-of-thought 복원 주장
- 원격 multi-user SaaS 운영 기능 (MVP 범위 외)

예외: `POST /v1/export-bundles`(미구현, 계획만)는 owner-only local export이며 외부 write가 아니다.

## Self-check before every commit

1. 이 변경을 잠그는 새 test가 있는가? 없다면 doc-only 변경인가?
2. 새 attribute/field/behaviour 주장에 docs 인용 또는 real-fixture assertion이 붙어 있는가?
3. UI 변경이 포함되면 브라우저 smoke를 끝냈는가?
4. 단일 사례를 일반화한 statement는 없는가? ("대부분/항상/모두"는 표본 수 명시.)
5. 이전 commit log·주석·spec 중 위 검증으로 틀린 부분이 있으면 같은 commit에서 정정했는가?
6. PR 직전인가? 그렇다면 아래 **개선 루프**를 실행했는가?

## 개선 루프 — PR 전 필수

PR 전 반드시 세 루프를 실행해 이번 세션의 미분류를 해소한다. 셋 다 동형이다 — 스크립트로
후보를 surface → **보편 항목(보편 CLI·확장자, 공식/공개 plugin)은 이 PR에서 사전에 추가**
(승인+TDD, 별도 PR로 분리 금지 — "이 PR과 무관"은 보류 사유가 아니다) → **비보편만 보류**
(회사/개인 one-off, 세션 셋업·메타 도구, 셸 토크나이저 파편, 복구 불가 출력) → 재빌드 후
재확인해 루프를 닫는다. 미실행 PR 금지. 사전 추가는 소스 편집이라 read-only API 원칙과 무관.

```bash
cd webui && node scripts/untagged-bash.ts --all         # Bash/Read 미분류 → 사전 src/insight/event_tags.rs
cd webui && node scripts/unknown-verification.ts --all   # 검증 outcome=unknown → 파서 src/ingest/verification_run.rs
cd webui && node scripts/unidentified-plugins.ts --all    # MCP 미식별 → 사전 src/insight/event_tags.rs
```

상세·hint 분류·false-positive 주의는 `docs/implementation-notes.html`
(`#untagged-bash-loop`·`#unknown-verification-loop`·`#unidentified-plugins-loop`).

## Implementation Notes (지속 유지 의무)

`docs/` 사양을 구현하면서 `docs/implementation-notes.html`을 계속 갱신한다 — 설계 결정,
사양에서 의도적으로 벗어난 편차, 고려한 트레이드오프, 사용자 판단이 필요한 열린 질문.

## How to Read the HTML Specs

HTML이라 grep이 어렵다. 텍스트만 빠르게 보려면:

```bash
python3 -c "
import re, html, sys
t = open(sys.argv[1]).read()
t = re.sub(r'<(script|style)[^>]*>.*?</\1>', '', t, flags=re.S|re.I)
t = re.sub(r'<[^>]+>', '\n', t)
print(html.unescape(re.sub(r'\n\s*\n+', '\n', t)))
" docs/02_technical_architecture_spec.html
```

## Communication

- 한국어로 응답. 기술 용어·식별자는 원문 유지.
- 사양 충돌 발견 시 임의 결정 금지 — 어느 문서가 SSOT인지 확인.
