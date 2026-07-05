# implementation-notes 토픽 인덱스 (B-5, 2026-07-04)

`docs/implementation-notes.html`(append-only 원장)의 **토픽별 현재 진실**
포인터. 원장은 결정 이력의 SSOT로 불변이고, 이 인덱스는 "지금 무엇이
유효한가"를 에이전트가 싸게 찾는 입구다. **항목 추가 시 이 인덱스의 해당
토픽도 같은 PR에서 갱신한다** (CLAUDE.md Implementation Notes 절).

앵커 사용법: `docs/implementation-notes.html#<anchor>` — 텍스트 추출은
CLAUDE.md의 python 스니펫.

| 토픽 | 현재 진실 (최신 결정) | 이력 앵커 |
|------|----------------------|-----------|
| 데이터 모델·뷰 계층 | `#event-first-redesign` (★ event-first 재설계) | `#views-without-graph` · `#conversation-anchored-window` · `#graph-removal` · `#span-dedup` · `#facet-correlation` |
| Detector·L1 Signal | `#final-state-mismatch-removal-2026-07-03` (L1 4종, 의미 판별은 LLM) | `#judge-removal` · `#finding-to-signal` · `#episode-removal` · `#detector-improvement-loop` · `#utf8-extractor-fixes` |
| SessionMetrics·series | `#metrics-cache-2026-07-04` (인메모리 캐시, §10.1 실측) | `#behavioral-metrics-plan3a` · `#self-improvement-loop-2026-06-12` |
| 태깅 루프·인프라 | `#tagging-infra-2026-07-04` ($() 편평화·무확장 규칙·게이트) | `#untagged-bash-loop` · `#noise-disposition-2026-06-30` · `#tagging-loop-2026-07-03` · `#tagging-loop-2026-07-04` |
| verification 파싱 | `#verification-tsc-2026-07-04` (tsc 승격, 패턴 17) | `#unknown-verification-loop` |
| Teammate·Subagent 관측 | `#teammate-followups-2026-07-04` (B-6 종결: preview·북엔드·agent-setting·표본 2) | `#teammate-observability-2026-07-03` · `#teammate-in-session-2026-07-03` · `#bg-subagent-hairline-gutter-2026-06-14` · `#task-notification-sync-2026-06-14` |
| WebUI replay·목록 | `#session-filtering-2026-07-04` (PR-1: 4축 서버 필터·FilterBar·flat 모드·점프 규칙) | `#session-list-perf-2026-06-29` · `#scroll-scrollbar-fix` · `#autoscroll-model` · `#pr33-regression-review` · `#tool-metrics-transcript-fallback` |
| 툴팁 카피·i18n 게이트 | `#cost-tooltip-dynamic-2026-07-05` (비용 툴팁 동적 조립; 함수형 tip-fragment 키는 tipStyle 게이트 우회 — 손 검증) | — |
| 프로젝트 대시보드 | `#dashboard-feedback-2026-07-04` (전면 개편: ECharts 2탭·코호트 랭킹·instruction 관측·B-12/13/14) | `#project-dashboard-2026-07-04` · `#dashboard-shadcn-2026-07-04` |
| MCP 표면 | `#mcp-digest-events-2026-07-04` (11종: structuredContent·events·digest) | `#mcp-parity-detector-config-2026-07-03` · `#dogfood-retrospect-2026-06-12` |
| export·거버넌스 | `#export-bundle-2026-07-04` (POST /v1/export-bundles) + `#prd-09-decisions-2026-07-04` (§09 3건 종결) | `#full-retention` · `#slice-18-deviations` · `#slice-19-deviations` |
| telemetry fold | `#telemetry-fold-group-a` · `#telemetry-fold-group-bc` | — |

## 이원화 구조 (B-5 결정)

- **원장**: `implementation-notes.html` — append-only, 앵커 불변. 결정
  이력의 SSOT.
- **현재 진실 인덱스**: 이 파일 — 토픽 → 최신 앵커.
- **열린 질문/보류**: `BACKLOG.md` — 인수인계 가능한 작업 단위.
- **마크다운 저작 + HTML 생성 검토(기각)**: 원장을 마크다운으로 재저작하면
  기존 앵커 링크(BACKLOG·커밋 메시지·스킬이 참조)가 깨지고 생성 파이프라인
  유지비가 든다. 에이전트 소비 문제는 이 인덱스 + BACKLOG 분리로 해소 —
  원장 자체를 읽는 일은 앵커 단위 추출로 충분하다.
