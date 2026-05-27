# CLAUDE.md — What's in My Claude Code

## Project Overview

Claude Code 실행을 **로컬에서** 관측하여 OTel-first 실행 그래프와
**evidence-linked insight**를 만드는 로컬 서비스. 사용자는 chat transcript가
아닌 **execution replay**로 "무엇이, 왜 그렇게 됐는가"를 본다.

- 산출물: 개선 patch가 아니라 **evidence-linked insight resource**
- 외부 접근: Pull API / MCP Streamable HTTP — **read-only**
- 기본 네트워크: `127.0.0.1` binding

## Status

- 현재 단계: **M3 완료 / M5 slice-14 완료** — slice-1~14 완료 (transcript / OTel / hook /
  ObservedEvent + telemetry facet · 기본 graph builder · WebUI replay ·
  SSE live updates · windowed event buffer · filesystem-source removal +
  transcript-only file lineage + Files lane graph linkage (slice-10a) ·
  VerificationRun ingest + graph edges + Pull API (slice-11) ·
  Episode segmentation state machine + golden + Pull API (slice-12) ·
  Causal-edge inference v1 — three inferred-edge rules (slice-13) ·
  **Insight Engine v1 — L1 deterministic extractors + /v1/findings* API (slice-14)**).
- 남은 작업의 **계획은 잠겼음**:
  `docs/superpowers/specs/2026-05-27-witmcc-remaining-milestones-roadmap.md` +
  per-slice design specs + per-slice TDD plans (`2026-05-27-witmcc-slice11..19-*`).
  Insight 엔진 L1/L2 분리 설계는
  `2026-05-27-witmcc-insight-engine-architecture.md`. UX 재설계는 마일스톤 밖
  별도 epic (`2026-05-27-witmcc-ux-redesign-epic.md`).
- 잔여 마일스톤: M5 Insight (~~slice-14 L1~~ ✓ · slice-15 L2 infra · slice-16 L2
  categories), M6 MCP (slice-17), M7 Hardening (slice-18 Redaction · slice-19
  Auth/Retention).
  (M3 완료: ~~slice-11 VerificationRun~~ ✓ · ~~slice-12 Episode~~ ✓ · ~~slice-13 Causal-edge~~ ✓)
- 구현 상세: `docs/implementation-notes.html` 의 slice별 섹션.
- **운영 주의 (slice-14):** `finding` 테이블 추가 (migration 0008). 기존 dev DB는
  `witmcc init-db`로 재생성 후 재ingest 필요.
- **운영 주의 (slice-13):** `graph_edge` 테이블에 `inference_rule_id`, `confidence` 컬럼 추가 (migration 0007). 기존 dev DB는 `witmcc init-db`로 재생성 후 재ingest 필요.
- **운영 주의 (slice-12):** `episode` 테이블 추가 (migration 0006). 기존 dev DB는
  `witmcc init-db`로 재생성 후 재ingest 필요. rebuild_session이 자동으로 episode rows를 생성함.
- **운영 주의 (slice-11):** `verification_run` 테이블 추가 (migration 0005). 기존 dev DB는
  `witmcc init-db`로 재생성 후 재ingest 필요. `witmcc ingest --all` 시 세션당 VR 자동 추출.
- **운영 주의 (slice-10a):** 기존 dev DB (`.witmcc.sqlite*`)는 폐기 후
  `witmcc init-db`로 재생성 필요. sqlx migration hash 변경됨.

## Document Map

작업 시작 전 관련 사양서를 먼저 읽는다. 모두 자기완결 HTML(외부 JS/CSS 없음).

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
