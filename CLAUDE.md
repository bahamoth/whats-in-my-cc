# CLAUDE.md — What's in My Claude Code

## Project Overview

Claude Code 실행을 **로컬에서** 관측하여 OTel-first 실행 그래프와
**evidence-linked insight**를 만드는 로컬 서비스. 사용자는 chat transcript가
아닌 **execution replay**로 "무엇이, 왜 그렇게 됐는가"를 본다.

- 산출물: 개선 patch가 아니라 **evidence-linked insight resource**
- 외부 접근: Pull API / MCP Streamable HTTP — **read-only**
- 기본 네트워크: `127.0.0.1` binding

### 현시점 설계 결정 — view / graph 분리 (as of 2026-06-04, 폐기·대체 가능)

위 "OTel-first 실행 그래프"는 graph가 **모든 뷰의 backing model이라는 뜻이 아니다.**
메세지·디테일·raw **뷰**는 `ObservedEvent` + correlation 키로 직접 그리고, graph는
**causal-edge inference + `/graph` Pull API/MCP 전용**이다. 사양서(00~04)는 옛
graph-backed 모델을 서술하므로 그대로 따르지 말 것.

> **현시점 결정**이며(이전 episode 분류·graph-backed 뷰가 폐기됐듯) 폐기·대체될 수 있다.
> 상세·근거·glossary·영향 사양·재설계 시 갱신 지점: **`docs/implementation-notes.html#event-first-redesign`**.

## Status

- **단계: MVP EXIT (2026-05-27). slice-1~19 완료, M3·M5·M6·M7 closed, AC-1~7 green.** slice별 구현 상세·이력은 `docs/implementation-notes.html`(§33~36 + `#event-first-redesign` + `#episode-removal`)와 git history.
- **남은 계획:** UX 재설계 epic (`2026-05-27-witmcc-ux-redesign-epic.md`) — 마일스톤 밖 별도 트랙.
- **인증 default = `--auth off`** (단일 사용자 dev, DEV-S19-08) — 브라우저로 그대로 접속. 켜려면 `witmcc serve --auth on`: 모든 `/v1/*` + `/mcp` 요청에 `Authorization: Bearer <token>` 필요. Token 위치 macOS `~/Library/Application Support/witmcc/token` · Linux `~/.config/witmcc/token` (0600). retention sweep는 `witmcc serve --retention-profile default`.
- **dev DB 재생성 규칙:** migration 변경(현재 최신 `0017`) 시 `witmcc init-db` + 재ingest 필요. payload 필드(`tool_call.tool_name`, `assistant_message.model` 등)도 JSON BLOB이라 schema migration 없이 추가되므로 기존 이벤트엔 없음 — 재ingest해야 채워진다.

## Document Map

작업 시작 전 관련 사양서를 먼저 읽는다. 모두 자기완결 HTML(외부 JS/CSS 없음).

> **⚠ event-first 뷰 재설계(2026-06-04, PR #33) 반영 주의:** 아래 01/02/03/04 문서는 메세지/디테일/raw 뷰가 **graph node/facet 기반**이라고 서술하지만, 실제 구현은 **`ObservedEvent` + correlation 키 기반**으로 바뀌었다. 각 문서에 해당 지점 callout이 있으며, 정식 기록은 `docs/implementation-notes.html#event-first-redesign`. 뷰/window/events-API/facet 관련 작업 전 반드시 이 섹션을 먼저 읽을 것.

| 작업 영역 | 먼저 읽을 문서 |
|----------|--------------|
| 제품 요구·범위·non-goals | `docs/00_prd_revised.html` |
| UX·screen·replay·why panel | `docs/01_product_design_spec.html` |
| 파이프라인·store·OTel 연동 | `docs/02_technical_architecture_spec.html` |
| 스키마 (ObservedEvent, Graph, Finding…) | `docs/03_data_model_spec.html` |
| HTTP / MCP endpoint 계약 | `docs/04_api_mcp_spec.html` |
| Collection profile·redaction·export | `docs/05_security_governance_spec.html` |
| Milestone·acceptance criteria | `docs/06_mvp_execution_plan.html` |

`docs/index.html`은 위 문서의 포털.

## Working Principles (반드시 지킬 것)

- **OTel-first**: `trace_id` / `span_id`는 optional metadata가 아니라
  correlation_keys·telemetry facet의 1급 키. 나중에 붙이는 식으로 schema 변경 금지.
- **Source-preserving**: normalized object는 항상 raw source reference를 가진다.
  unknown field는 raw payload에 보존.
- **Evidence-linked**: `Finding` / `RootCauseHypothesis` / `QualitySummary`는
  `evidence_refs` 없이 만들지 않는다.
- **No annotation model**: 외부에서 finding/resource에 correction·label·status를
  쓰는 API는 정의·구현하지 않는다.
- **Local-first**: 기본 bind `127.0.0.1`. `0.0.0.0`은 explicit setting일 때만.
- **Schema versioning**: 모든 top-level object는 `schema_version` + provenance.
- **TDD red 우선**: 어떤 모듈/함수/UI 변경이든 *실패하는* 테스트를 먼저 작성하고
  그 테스트가 빨강을 보이는 것을 확인한 다음에 구현으로 넘어간다.
  test가 후행이거나 omit된 commit은 만들지 않는다. 단순 doc 변경은 예외.
- **Real-data anchoring**: 외부 source(transcript JSONL / OTLP / hook stdin / git2 등)의
  attribute·field 의미에 대한 주장은 *둘 중 하나*에 의해 잠긴다.
  (a) 공식 docs URL과 인용,  (b) `tests/fixtures/**/real/`에 동결된 실 payload에 대한
  invariant assertion. 둘 다 없는 가정은 spec/commit log/주석에 적지 않는다.
  표본 1건으로 일반화하지 않는다 — 패턴인지 단일 케이스인지 명시.
- **UI는 브라우저 smoke 후 commit**: WebUI 변경은 `cargo build` + `vitest` 통과만으로는
  완료가 아니다. 사용자 환경에서 `witmcc serve` + 브라우저 navigation (claude-in-chrome
  도구) + 시각 검증까지 끝낸 뒤 commit. 가능한 한 매 incremental 변경마다 smoke.

## Self-check before every commit (의무 체크리스트)

1. 이 변경을 잠그는 새 test가 있는가? 없다면 doc-only 변경인가?
2. 새 attribute / field / behaviour 주장에 docs 인용 또는 real-fixture assertion이 붙어 있는가?
3. UI 변경이 포함된다면 브라우저 smoke를 끝냈는가?
4. 단일 사례 관찰을 일반화한 statement는 없는가? ("대부분", "항상", "모두" 류는 표본 수 명시.)
5. 이전 commit log / 주석 / spec 중 위 검증으로 잘못 판명된 부분이 있다면 같은 commit에서 정정했는가?

## Tagging loop — 구현 완료 시 (Bash/Read 분류 강화 제안)

작업(기능/슬라이스/요청)이 **완료된 시점**에, 그 실행에서 아직 분류되지 않은 Bash 명령을
확인하고 태깅 규칙을 어떻게 강화할지 **제안**한다(자율 태깅 개선 루프를 닫는다). 규칙 추가는
소스 편집이라 read-only API 원칙과 충돌하지 않는다.

1. `cd webui && node scripts/untagged-bash.ts --all` 실행(특정 세션만 보려면 끝에 `<sessionId>`).
   stdout이 **깨끗한 JSON**(npm 배너 없음 — `npm run`은 stdout에 배너를 찍으므로 node 직접 실행).
   Node 22+ 네이티브 타입 스트리핑으로 실행, 백엔드·vite-node 불필요. `/v1/sessions`는 최신순이라
   방금 작업한 세션이 맨 위. read-only Pull API + 프론트 `collectUntagged`(SSOT) 재사용.
   출력: `[{token,count,sample,eventId,sessionId,hint}]` (count 내림차순).
2. count 높은 untagged 토큰부터, 각 row의 `sample`을 근거로 분류가 타당한지 검토한 뒤,
   `hint`가 가리키는 대로 `webui/src/components/replay/stream/eventTags.ts`에 어떤 태그를
   추가할지 **제안**한다: 일반 첫 토큰은 `BASH_FIRST_TOKEN_TAGS`, `git` 서브커맨드는
   `GIT_SUBCOMMAND_TAGS`, Read 확장자는 `READ_EXT_TAGS`. (파괴적 명령은 이미 `DESTRUCTIVE_FIRST_TOKENS`.)
3. 사용자 승인 후 규칙을 추가하고 CLI를 다시 실행해 untagged가 줄었는지 확인 — 루프가 닫힌다.
   (분류 변경이므로 `eventTags.test.ts`에 잠그는 테스트를 함께 둘 것 — TDD 원칙.)

상세·설계 근거: `docs/implementation-notes.html#untagged-bash-loop`.

## Non-goals (절대 만들지 말 것)

- Claude Code 설정 / hook / command / skill / memory 변경
- 개선 patch / 설치 스크립트 자동 생성
- 외부 service-to-service correction·label·status write
- hidden reasoning / private chain-of-thought 복원 주장
- 원격 multi-user SaaS 운영 기능 (MVP 범위 외)

예외: `POST /v1/export-bundles`는 owner-only local export action이며 외부 write가 아니다.

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
