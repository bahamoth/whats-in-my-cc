# WitMCC UX 재설계 — 세션 인수인계 (2026-06-15)

다른 세션이 이 작업을 이어받기 위한 문서. **설계의 SSOT는
`2026-06-14-witmcc-ux-redesign-design.md`** (먼저 읽을 것). 이 문서는 진행 상태 +
남은 일 + 작업 방식만 정리한다.

## 목표 (사용자 지시)
> 모든 변경 계획을 완벽히 작성하고, 구현을 마무리한 후 PR 까지 완성할 것.

전 화면 UX 재설계를 **A+B 하이브리드** 디자인 언어로 구현하고 **단일 PR**로 완성.

## 브랜치 / PR
- 브랜치 **`feat/ux-redesign-ab`**, `main@0.7.0` 위에 stack (clean — main이 ancestor).
  - 주의: 세션 중 `feat/workflow-grouping`(구 PR #60)이 main에 머지·0.7.0 릴리스되고
    삭제됨. 그 그룹핑 작업(workflow_run_id·WorkflowGroup·BatchGroup)은 이제 base(main)에 있음.
- **드래프트 PR #62** — https://github.com/bahamoth/whats-in-my-cc/pull/62
  (단일 PR에 슬라이스 누적, 계속 push). 머지는 rebase-linear.

## 잠긴 결정 (brainstorming + AskUserQuestion)
1. **범위**: 전 화면 통합 재설계 (세션 목록·KPI·스트림·detail·analysis).
2. **강도**: 디자인 언어 발전 (현 다크 토큰 위에 새 언어).
3. **방향**: **A+B 하이브리드** = B(좌측 시간축 spine + 노드 + 병렬 레인) × A(하이라인
   제거·표면 elevation·여백·절제된 색·조판).
4. **배치/서브에이전트**: 요약 상시 표시(접기 없음) + 내부는 **인라인 sub-timeline 드릴**
   (option 1). 단일 에이전트 = 래퍼 없음, 실행 중 = 자동 진행 표시, ≥4 동시 = 밀집 spine+"+N".
5. **단일 항목 접힘 전면 제거**.
6. **spine이 gutter를 흡수** (시간축 spine이 유일한 좌측 시스템, bg 서브에이전트는 레인으로 분기).
7. **백엔드 포함 풀스택** (S6 세션목록 필드·S8 KPI series는 Rust 변경 + 재ingest).
8. **단일 PR**(지금 열어 계속 push) — 헌장 §7 "slice별 다중 PR" 권고를 사용자가 override.

## 완료 (6커밋, 모두 TDD green + 브라우저 스모크)
| commit | 내용 |
|---|---|
| `7923786` docs | 재설계 디자인 스펙 |
| `52c5f3f` fix | 단일 이벤트 ActivityStack 인라인 (events.length===1 → 토글 없음) |
| `66da228` feat | 토큰 `--wimcc-radius-sm/md/lg`·`--wimcc-elev-1/2` + MessageCard 버블 A 톤 |
| `10e59fd` feat | **통합 시간축 spine** — BgGutter가 연속 spine line + kind 노드 + bg 레인을 spine 우측으로 재앵커(14→30px). ConversationStream `rowKind()` |
| `af8731c` fix | correlation ID 칩 클릭 시 전체 ID 복사(InsightTab `CopyChip`) |
| `f029e45` fix | 단일 에이전트 워크플로우 평탄화(WorkflowGroup, N=1 시 래퍼·간트·이중 chevron 제거 — **모델 불변, 렌더링만**) |

## 추가 완료 (2026-06-15 이어받기, +3커밋 push됨)
- `afb13fd` feat: **spine 좌측 시간축 라벨**(HH:MM ruler) — 셀 30→58px, format.ts `clockLabel`,
  streamModel `rowTimeMs` export. **B 트레이스 타임라인 완성** ✓ 스모크.
- `f029e45` fix: 단일 에이전트 워크플로우 평탄화(WorkflowGroup, 모델 불변·렌더링만).
- `3ee9667` feat: 병렬 배치 상시 미니 간트(WorkflowGroup 간트 재사용, `batch-lane`).

**발견(중요)**: 좌상단 `◐`은 테마 토글이 아니라 **nav rail의 Sessions 링크**(AppShell). 라이트모드는
UI 토글 없이 **`prefers-color-scheme`** 기반 — S10 라이트 검증은 브라우저 emulate 필요. 또
`fb6b8e3a`의 21:33 "Agent…" 행들은 **BatchGroup으로 미검출**(standalone SubagentGroup) — 실제
"병렬 배치" 스모크 세션을 따로 찾아야 함(653ea169도 workflow-fanout 위주라 batch 적을 수 있음).

## 남은 일 (PR #62 체크리스트, 우선순위·의존성)
- **S5 FanPanel 잔여**: 현재 BatchGroup/WorkflowGroup은 요약(간트+종합) 상시 + 펼치면 자식
  SubagentGroup(각자 드릴) — 기능적으로 근접. 미완: 레인 클릭 시 **그 에이전트만 인라인 드릴**
  (현재는 그룹 전체 펼침). 큰 reorg + 디자인 민감 → 사용자 눈 + 실제 배치 세션 필요.
  목업 `webui/public/batch-states.html` 상태①②③ 참고.
- **S6 세션 목록** (풀스택): `/v1/sessions` 응답에 `slug`·첫 사용자메시지 미리보기·`model`(dominant)
  ·`project` 추가(Rust) + SessionListPage 재설계(슬러그·미리보기·상대시간·검색·반응형).
  `slug`는 transcript payload에 존재(Raw에서 실측). real-fixture로 잠그고 재ingest.
- **S7 detail 패널**: HOW를 `LLM 동작`/`토큰`/`검증`/`비용` 소제목으로 묶고 행마다 provenance pill.
  Signal은 `/v1/sessions/:id/signals` 소비(판단 색 금지). WhatSection 2000자 "전문" 앵커.
- **S8 KPI 스파크라인·베이스라인** (풀스택): intra-session 추이=`/v1/sessions/:id/turns` 집계,
  베이스라인=`/v1/sessions/:id/fingerprint` 코호트. **비용 카드는 추정 유지**(측정-비용 swap 보류, 메모리).
- **S9 Analysis 재설계**: 토글 아이콘화, detector 분포, 단일 signal 인라인, max-height 400px 제거.
- **S10**: 반응형(800px 정규화·레일/우측슬롯 내로우·세션테이블 카드스택)·라이트모드(토큰 override
  검증)·a11y(아이콘 aria-label·범례)·키보드(j/k·z·e·/).
- ~~spine 시간 라벨~~ ✓ 완료(`afb13fd`).

## 작업 방식 (반드시 지킬 것)
- **TDD red 우선** (superpowers:test-driven-development) → **브라우저 스모크 후 commit** (CLAUDE.md).
- **vitest는 `webui/`에서 실행**: `cd webui && npx vitest run <path>`. (repo 루트서 실행하면 jsdom
  환경 미적용 → "document is not defined".)
- **커밋에 AI footer 금지** (Co-Authored-By/Generated — 프로젝트 PreToolUse 훅이 차단, 메모리).
- **conventional commit** (release-please가 버전 결정). `Cargo.toml`·`webui/package.json` 버전 손대지 말 것.
- **스모크 환경**: `WIMCC_DB=.wimcc.sqlite target/release/wimcc serve --port 7878 --auth off` 실행 중 +
  Vite dev `:5173`(라이브 소스, HMR). 정적 스모크 세션 **`00fae5d9`**(active 아님; 단일이벤트·스캐폴드·
  메시지 풍부, 서브에이전트 그룹 없음). active 세션은 live-mutate라 피할 것.
- **백엔드 변경 시**: `wimcc init-db` + 재ingest, CI는 cargo fmt/clippy(-D warnings)/test.
- **read-only 원칙·fact-only 색(spec §6.3)·OTel-first·evidence-linked** 유지. Signal엔 severity/confidence 없음.

## 참고 자료
- 설계 SSOT: `docs/superpowers/specs/2026-06-14-witmcc-ux-redesign-design.md`
- 헌장: `2026-05-27-witmcc-ux-redesign-epic.md` (§3 Signal 모델 precondition, §5 no-inheritance)
- 목업(로컬 untracked, Vite `:5173/<name>`서 렌더): `webui/public/redesign-directions.html`(방향 3안)
  · `redesign-fullscreen.html`(전 화면 A+B) · `batch-states.html`(배치 상태 3종).
  *주의: 목업은 dist에 안 싣기 위해 미커밋 — 다른 머신 체크아웃이면 부재할 수 있음(스펙으로 재현 가능).*
- 코드 진입점: `streamModel.ts`(:339 scaffold N≥2, :1131 wf, :1144 batch N≥2, :718 streamItemTime),
  `BgGutter.tsx`(=spine cell), `ConversationStream.tsx`(rowKind/rowEventId), `InsightTab.tsx`(CopyChip),
  `WorkflowGroup.tsx`(단일 평탄화), `tokens.css`(진화 토큰).
