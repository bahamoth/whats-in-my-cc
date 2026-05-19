# CLAUDE.md — What's in My Claude Code

## Project Overview

Claude Code 실행을 **로컬에서** 관측하여 OTel-first 실행 그래프와
**evidence-linked insight**를 만드는 로컬 서비스. 사용자는 chat transcript가
아닌 **execution replay**로 "무엇이, 왜 그렇게 됐는가"를 본다.

- 산출물: 개선 patch가 아니라 **evidence-linked insight resource**
- 외부 접근: Pull API / MCP Streamable HTTP — **read-only**
- 기본 네트워크: `127.0.0.1` binding

## Status

- 현재 단계: **M0 (Spec freeze)** — 코드 없음, `docs/`에 사양서만
- 다음 단계: M1 ingestion (transcript / OTel / hook / file·git 수집기)

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
