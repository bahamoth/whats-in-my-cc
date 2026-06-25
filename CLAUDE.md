# CLAUDE.md — What's in My Claude Code

## Project Overview

Claude Code 실행을 **로컬에서** 관측하여 **event-first 실행 기록**(`ObservedEvent` +
correlation 키)과 **evidence-linked Signal**을 만드는 로컬 서비스. 사용자는 chat
transcript가 아닌 **execution replay**로 "무엇이, 왜 그렇게 됐는가"를 본다.

- 산출물: 개선 patch가 아니라 **deterministic L1 Signal + 온디맨드 SessionMetrics**
- 외부 접근: Pull API / MCP Streamable HTTP — **read-only**
- 기본 네트워크: `127.0.0.1` binding
- 설계 변경 이력의 SSOT는 git history + `docs/implementation-notes.html`

## Serena usage

When working on this repository, prefer Serena's symbol-level tools before broad file reads.

- Use Serena for symbol lookup, reference search, and targeted edits.
- Before modifying code, inspect relevant symbols and references.
- Avoid reading entire files unless symbol-level context is insufficient.
- For refactoring, prefer semantic rename/edit operations over raw text replacement.
- After changes, show the diff and run the relevant tests when feasible.

## 모델 오케스트레이션

작업을 하위 작업으로 분해해 적절한 모델에 위임한다. 라우팅은 아래 다섯 축 중
**가장 높은 등급 하나라도 해당되면 상위 모델을 선택**한다(평균 내지 않는다).

| 축          | 하위 (Haiku) | 중간 (Sonnet) | 상위 (Opus)            |
| ----------- | ------------ | ------------- | ---------------------- |
| 추론 깊이   | 단일 단계    | 국소 다단계   | 상호의존 다단계·트레이드오프 |
| 명세 명확도 | 완전 확정    | 대체로 명확   | 해석·설계 판단 필요    |
| 영향 범위   | 로컬·가역    | 단일 모듈     | 교차 절단·복구비용 큼  |
| 컨텍스트 폭 | 파일 1개     | 소수 파일     | 다수 모듈·시스템       |
| 정형성      | 패턴 반복    | 일부 신규     | 새로운 문제            |

### 기본값
- 계획·작업 분해·최종 검토는 오케스트레이터(Opus)가 직접 수행한다.
- 위임 가능한 하위 작업은 위 기준의 최저 티어에 배정한다.
- 티어가 애매하면 한 단계 낮춰 시도하고, 막히면 에스컬레이션한다.

### 에스컬레이션 규칙
- 하위 모델이 (a) 예상 밖 모호함, (b) 명세에 없는 설계 결정, (c) 동일 작업 2회 연속 실패 중 하나라도 만나면 즉시 상위 티어로 올린다.
- 보안·인증·결제·마이그레이션·삭제 등 비가역 작업은 티어 판정과 무관하게 Opus.

### 위임 시 명시할 것
- 입력/기대 출력, 건드릴 파일 범위, 완료 기준(테스트·검증 방법)을 함께 전달한다.


## Status

> 진행 중인 작업만 적는다. 완료 즉시 삭제하고, 항구 규칙은 `Operations`·`Working Principles`로 이전한다.
> 이 섹션이 길어지면 분류가 잘못됐다는 신호다 — 완료분이 안 지워졌거나, 항구 규칙이 잘못 들어왔다.

- **In-flight:** 자기개선 지표 트랙 — in-flight 자기조절 affordance. 상세·이력은 `docs/implementation-notes.html` + git history.

## Document Map

작업 시작 전 관련 사양서를 먼저 읽는다. 모두 자기완결 HTML(외부 JS/CSS 없음).

| 작업 영역 | 먼저 읽을 문서 |
|----------|--------------|
| 제품 요구·범위·non-goals | `docs/00_prd_revised.html` |
| UX·screen·replay·insight strip·분석 패널 | `docs/01_product_design_spec.html` |
| 파이프라인·store·OTel 연동 | `docs/02_technical_architecture_spec.html` |
| 스키마 (ObservedEvent, Signal, VerificationRun, Metrics…) | `docs/03_data_model_spec.html` |
| HTTP / MCP endpoint 계약 | `docs/04_api_mcp_spec.html` |
| Collection profile·redaction·export | `docs/05_security_governance_spec.html` |

`docs/index.html`은 위 문서의 포털.

## Working Principles (반드시 지킬 것)

데이터 모델 설계 원칙(OTel-first · source-preserving · evidence-linked · no-annotation-model ·
schema-versioning · deterministic-L1)의 SSOT는 `docs/03_data_model_spec.html` §01이다 —
여기서 재진술하지 않는다. 중복은 곧 드리프트다(실제로 이 문서의 OTel-first 문구가 스펙과 어긋나 있었다).
스키마/데이터 모델 작업 전 그 문서를 읽는다(Document Map).

아래는 스펙에 없는, 매 세션 지켜야 할 *작업* 규칙이다.

- **측정과 판별 분리**: 측정·집계·결정론 detector는 wimcc가 한다. "교정성 메시지" 류 *의미 판별*은
  lexical 휴리스틱으로 detector에 넣지 말고 LLM 온디맨드(retrospect 스킬)에 맡긴다.
- **Local-first**: 기본 bind `127.0.0.1`. `0.0.0.0`은 explicit setting일 때만.
- **TDD red 우선**: 어떤 모듈/함수/UI 변경이든 *실패하는* 테스트를 먼저 작성하고
  그 테스트가 빨강을 보이는 것을 확인한 다음에 구현으로 넘어간다.
  test가 후행이거나 omit된 commit은 만들지 않는다. 단순 doc 변경은 예외.
- **Real-data anchoring**: 외부 source(transcript JSONL / OTLP / hook stdin / git2 등)의
  attribute·field 의미에 대한 주장은 *둘 중 하나*에 의해 잠긴다.
  (a) 공식 docs URL과 인용,  (b) `tests/fixtures/**/real/`에 동결된 실 payload에 대한
  invariant assertion. 둘 다 없는 가정은 spec/commit log/주석에 적지 않는다.
  표본 1건으로 일반화하지 않는다 — 패턴인지 단일 케이스인지 명시.
- **UI는 브라우저 smoke 후 commit**: WebUI 변경은 `cargo build` + `vitest` 통과만으로는
  완료가 아니다. 사용자 환경에서 `wimcc serve` + 브라우저 navigation (claude-in-chrome
  도구) + 시각 검증까지 끝낸 뒤 commit. 가능한 한 매 incremental 변경마다 smoke.

## Operations

- **CI/CD·릴리스:** 모든 PR에서 GitHub Actions CI — vitest+SPA build 후 그 dist로
  `cargo fmt --check`·`clippy -- -D warnings`·`cargo test`. 릴리스는 release-please:
  main의 conventional commit이 릴리스 PR로 누적되고, 머지 시 `vX.Y.Z` 태그·CHANGELOG·
  바이너리(linux x86_64 · macOS arm64) 업로드. `Cargo.toml`·`webui/package.json`·
  `package-lock.json` 버전은 자동 bump — **손으로 수정 금지**. commit type이 버전을
  결정하므로 conventional commit 준수.
- **인증:** default `--auth off`(단일 사용자 dev) — 브라우저로 그대로 접속. 켜려면
  `wimcc serve --auth on`: `/v1/*`+`/mcp`에 `Authorization: Bearer <token>` 필요
  (`/v1/stream`·collectors·SPA는 예외). Token 위치 macOS `~/Library/Application Support/wimcc/token`
  · Linux `~/.config/wimcc/token` (0600, `dirs::config_dir()` 기준).
  retention sweep는 `wimcc serve --retention-profile default`.
- **dev DB 재생성:** migration 추가 시 원칙적으로 `wimcc init-db` + 재ingest. 단 컬럼 추가 후
  serve/ingest startup에서 backfill하는 migration(예: 0023 `agent_id`, 0024 `workflow_run_id`)은
  init-db·재ingest 불필요 — backfill 여부는 각 migration 파일 주석이 명시한다(현행 목록은 `migrations/`).
  payload 필드(`tool_call.tool_name`·`assistant_message.model` 등)는 JSON BLOB이라 migration 없이
  추가되므로 기존 이벤트엔 없음 — 재ingest해야 채워진다.

## Self-check before every commit (의무 체크리스트)

1. 이 변경을 잠그는 새 test가 있는가? 없다면 doc-only 변경인가?
2. 새 attribute / field / behaviour 주장에 docs 인용 또는 real-fixture assertion이 붙어 있는가?
3. UI 변경이 포함된다면 브라우저 smoke를 끝냈는가?
4. 단일 사례 관찰을 일반화한 statement는 없는가? ("대부분", "항상", "모두" 류는 표본 수 명시.)
5. 이전 commit log / 주석 / spec 중 위 검증으로 잘못 판명된 부분이 있다면 같은 commit에서 정정했는가?
6. `## Status`에 완료된 항목이 남아 있거나 항구 규칙(운영·설계)이 섞여 있지 않은가? 완료분은 삭제,
   항구 규칙은 `Operations`/`Working Principles`로 이전한다 — Status는 in-flight만.
7. PR을 올리기 직전인가? 그렇다면 `Tagging loop`·`Verification unknown loop`·`Plugin/MCP identification loop`을
   실행해 untagged/unknown/미식별을 해소하거나 명시적 보류 사유를 남겼는가? (미실행 상태로 PR 금지.)

## Tagging loop — PR 전 필수 (Bash/Read 분류 강화)

**PR을 올리기 전 반드시 실행한다.** 이 세션 실행에서 아직 분류되지 않은 Bash 명령을 surface하고,
타당한 것은 태깅 규칙으로 추가(아래 3번 — 승인+TDD), 나머지는 명시적 보류 사유를 남긴다. 미실행
상태로 PR을 올리지 않는다 — 매 세션 태깅 개선 루프를 닫는 것이 목적이다. 규칙 추가는 소스 편집이라
read-only API 원칙과 충돌하지 않는다.

1. `cd webui && node scripts/untagged-bash.ts --all` 실행(특정 세션만 보려면 끝에 `<sessionId>`).
   stdout이 **깨끗한 JSON**(npm 배너 없음 — `npm run`은 stdout에 배너를 찍으므로 node 직접 실행).
   Node 22+ 네이티브 타입 스트리핑으로 실행, 백엔드·vite-node 불필요. `/v1/sessions`는 최신순이라
   방금 작업한 세션이 맨 위. read-only Pull API + 프론트 `collectUntagged`(SSOT) 재사용.
   출력: `[{token,count,sample,eventId,sessionId,hint}]` (count 내림차순).
2. count 높은 untagged 토큰부터, 각 row의 `sample`을 근거로 분류가 타당한지 검토한 뒤,
   `hint`가 가리키는 대로 **`src/insight/event_tags.rs`**의 사전에 어떤 태그를 추가할지
   **제안**한다: 일반 첫 토큰은 `BASH_FIRST_TOKEN_TAGS`, 멀티플렉서(git/cargo/npm/…)
   서브커맨드는 `TOOL_SUBCOMMAND_TAGS`, Read/Edit 확장자는 `EXT_OBJECT`.
   (분류기·어휘는 2026-06-12에 webui→core로 이전 — events 응답의 `tag` 필드와 turns의
   `tag_histogram`이 같은 verb.object 어휘를 쓰고, webui `eventTags.ts`는 표현·집계만 담당.)
3. 사용자 승인 후 규칙을 추가하고 `tests/event_tags.rs`에 잠그는 테스트를 함께 둔다(TDD).
   태그는 API 렌더 시점 계산이므로 **서버 재빌드·재기동 후** CLI를 다시 실행해 untagged가
   줄었는지 확인 — 루프가 닫힌다.

상세·설계 근거: `docs/implementation-notes.html#untagged-bash-loop`.

## Verification unknown loop — PR 전 필수 (파서 강화, untagged-bash의 형제)

**PR을 올리기 전 반드시 실행한다.** 검증 가드(test/build/lint/format)의 outcome이 `unknown`으로
남은 run을 모아 파서를 어떻게 강화할지 검토하고, 타당한 패턴은 추가(아래 — 승인+TDD), 나머지는
명시적 보류 사유를 남긴다. 미실행 상태로 PR을 올리지 않는다. tier-4 휴리스틱 추가는 소스 편집이라
read-only API 원칙과 충돌하지 않는다.

1. `cd webui && node scripts/unknown-verification.ts --all`(특정 세션만 보려면 끝에 `<sessionId>`).
   stdout이 **깨끗한 JSON**(npm 배너 없음 — node 직접 실행). read-only Pull API
   (`/v1/sessions/:id/verification-runs` + `/v1/events/:id/raw`)와 프론트 `collectUnknownVerification`
   (SSOT, `webui/src/lib/unknownVerification.ts`)를 재사용. 권위 파싱은 Rust에 있고
   스크립트는 후보 surfacing만 한다.
   출력: `[{commandKind, statusBasis, count, sampleCommand, sampleContentTail, hint}]` (count 내림차순).
2. `hint`별로 분류한다: `piped`인데 요약 텍스트가 보이면 `src/ingest/verification_run.rs`의
   `looks_like_success`/`looks_like_failure`에 패턴 추가 후보. `요약 없음`은 하위 필터(grep/wc)나
   tool_result 잘림으로 복구 불가. `disposition(...)`은 정상 unknown(조치 불필요). `exit basis인데
   미해소`는 exit-code 라인·요약 둘 다 미인식 — content tail을 보고 새 패턴 검토.
3. **주의(false-positive)**: cargo `test`도 빌드 단계에서 "Finished … profile"을 찍으므로
   build-success 패턴을 무분별 추가하면 실패한 테스트를 통과로 오판한다. 패턴은 command_kind로
   범위를 한정하고(예: build/build_check에서만 "Finished … target(s)"), `looks_like_failure`가
   먼저 검사됨을 활용한다. 추가 시 `verification_run.rs` 테스트에 잠그는 케이스를 함께 둘 것(TDD).
4. 사용자 승인 후 패턴을 추가하고 재ingest → 스크립트를 다시 실행해 unknown이 줄었는지 확인 — 루프가 닫힌다.

상세·설계 근거: `docs/implementation-notes.html#unknown-verification-loop`.

## Plugin/MCP identification loop — PR 전 필수 (MCP 태깅 강화, 위 둘의 형제)

**PR을 올리기 전 반드시 실행한다.** verb.object 태그가 아직 없는 MCP 도구(`tag.disposition=unmatched`)를
provenance와 함께 모아, community(official/public) plugin 도구는 태깅하고 personal/configured는 명시적으로
제외한다. 미실행 상태로 PR을 올리지 않는다. `MCP_SERVER_TOOL_TAGS` 추가는 소스 편집이라 read-only API
원칙과 충돌하지 않는다.

1. `cd webui && node scripts/unidentified-plugins.ts --all`(특정 세션만 보려면 끝에 `<sessionId>`).
   stdout이 **깨끗한 JSON**(npm 배너 없음 — node 직접 실행). read-only Pull API(events + `/v1/plugins`)만
   소비. provenance는 `/v1/plugins`(= `claude plugins list/marketplace list --json`, `src/plugins.rs`)에서
   온다 — wimcc 서비스는 인터넷을 타지 않는다.
   출력: `[{token, server, tool, count, provenance, plugin, sessionId, hint}]` (count 내림차순).
2. `hint`별로 분류한다: provenance `official`/`public`이면 `src/insight/event_tags.rs`의
   `MCP_SERVER_TOOL_TAGS`에 `server→tool` verb.object를 추가하는 후보(도구의 동작이 모호하면 마켓플레이스
   repo를 **WebFetch**해 확인 — 이 인터넷 조회는 루프를 도는 **에이전트**의 몫이지 스크립트/서비스가 아님).
   provenance `personal`(directory-source 마켓)·`configured`(직접 설정 MCP 서버)는 **태깅 제외** — 그대로
   unmatched로 두고 루프가 쫓지 않는다.
3. **모든 unmatched가 태깅 대상은 아니다**: 세션 셋업·메타 도구(예: serena `activate_project`)는 read/write
   /run 어디에도 안 맞으면 의도적으로 unmatched로 남긴다 — 억지로 태그하지 않는다. 추가하는 도구는
   `tests/event_tags.rs`에 잠그는 케이스를 함께 둘 것(TDD).
4. 사용자 승인 후 사전에 추가하고 **서버 재빌드·재기동** 후 스크립트를 다시 실행해 목록이 줄었는지 확인 — 루프가 닫힌다.

상세·설계 근거: `docs/implementation-notes.html#unidentified-plugins-loop`.

## Non-goals (절대 만들지 말 것)

- Claude Code 설정 / hook / command / skill / memory 변경
- 개선 patch / 설치 스크립트 자동 생성
- 외부 service-to-service correction·label·status write
- hidden reasoning / private chain-of-thought 복원 주장
- 원격 multi-user SaaS 운영 기능 (MVP 범위 외)

예외: `POST /v1/export-bundles`(미구현, 계획만 존재)는 owner-only local export action이며 외부 write가 아니다.

## Implementation Notes (지속 유지 의무)

`docs/` 사양서를 구현하면서 `docs/implementation-notes.html`을 계속 업데이트한다.
구현이 사양에서 벗어나거나 해석한 방식 등 사용자가 알아야 할 모든 것을 기록한다.

- **설계 결정**: 사양이 모호했던 부분에서 내린 선택
- **편차**: 사양에서 의도적으로 벗어난 부분과 이유
- **트레이드오프**: 고려한 대안과 선택 이유
- **열린 질문**: 사용자 판단이 필요한 것

## How to Read the HTML Specs

HTML이라 grep이 어렵다. 텍스트만 빠르게 보고 싶으면:

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

- 한국어로 응답. 기술 용어와 식별자는 원문 유지.
- 사양 충돌 발견 시 임의 결정 금지 — 어느 문서가 source of truth인지 확인.
