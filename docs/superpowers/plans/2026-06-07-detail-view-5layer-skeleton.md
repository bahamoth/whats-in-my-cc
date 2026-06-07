# Detail View 5-Layer Skeleton (Plan 2b) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`).

**Goal:** 디테일 뷰(InsightTab)를 공통 골격 5층으로 재구성한다 — **H. 헤더+correlation(원본/가공 배지) → ① WHAT(한 일) → ② HOW(지표) → ③ SIGNALS → ④ RAW**. 핵심은 **WHAT을 1층으로 끌어올리는 것**(현재 헤더 한 줄/Raw로 밀려 있음)과 원본/가공 시각 구분.

**Architecture:** `eventProvenance(kind)` util로 원본(ObservedEvent native: 파랑)/가공(derived: 보라) 판별 → 헤더 배지. `WhatSection` 신규 컴포넌트가 타입별 본문을 렌더(tool_call은 command + matched tool_result 출력; user는 prompt; assistant는 text; diff/verification은 patch/status). matched tool_result는 `SessionDetailPage`가 tool_use_id로 events에서 찾아 prop으로 전달(기존 `toolMetrics` 패턴 재사용). HOW(`EntityMetricsPanel`)·SIGNALS(`SignalsList`)·RAW(`RawTab`)는 유지하고 순서/래핑만 5층에 맞춤.

**Tech Stack:** TypeScript, React 18, Vitest, Vite.

**Spec:** `2026-06-07-detail-view-derived-metrics-design.md` §8.1(공통 골격 5층)·§8.2(타입별 매트릭스). brainstorm mockup: `.superpowers/brainstorm/.../detail-skeleton.html`.

**중요(메모리):** 이 plan은 디자인 변경이므로 **마지막에 사용자 브라우저 시각 확인(live lock)** 이 필수. 자동화 navigate는 사용자가 거부했으므로 controller가 serve를 띄워두고 **사용자가 직접 확인**한다.

---

## File Structure
- Create: `webui/src/components/replay/detail/eventProvenance.ts` — kind → 'native'|'derived' + 한국어 라벨
- Create: `webui/src/components/replay/detail/WhatSection.tsx` (+ `.module.css`) — 타입별 WHAT 본문
- Modify: `webui/src/components/replay/detail/InsightTab.tsx` (+ css) — 5층 통합
- Modify: `webui/src/components/replay/detail/DetailPanel.tsx` — `matchedResult` prop 추가
- Modify: `webui/src/routes/SessionDetailPage.tsx` — selected tool_call의 matched tool_result 계산·전달
- Test: `eventProvenance.test.ts`, `WhatSection.test.tsx`, InsightTab.test.tsx 갱신

---

## Task 1: eventProvenance util + 헤더 배지/correlation 칩

**Files:** `eventProvenance.ts`(+test), `InsightTab.tsx`(+css)

- [ ] **Step 1: 실패 테스트** `eventProvenance.test.ts`
```typescript
import { eventProvenance } from '../eventProvenance';
test('native vs derived', () => {
  expect(eventProvenance('tool_call').kind).toBe('native');
  expect(eventProvenance('assistant_message').kind).toBe('native');
  expect(eventProvenance('diff_hunk').kind).toBe('derived');
  expect(eventProvenance('verification_run').kind).toBe('derived');
});
```

- [ ] **Step 2: 구현** `eventProvenance.ts`
```typescript
// ObservedEvent.kind는 Claude Code 원본 관측(native). diff_hunk/verification_run/
// signal 등 wimcc 파생물은 derived. (spec §3 층위 — 원본/가공 구분)
const DERIVED = new Set(['diff_hunk', 'verification_run', 'signal']);
export function eventProvenance(kind: string): { kind: 'native' | 'derived'; label: string } {
  return DERIVED.has(kind)
    ? { kind: 'derived', label: '가공' }
    : { kind: 'native', label: '원본' };
}
```

- [ ] **Step 3: 테스트 통과** `cd webui && npx vitest run src/components/replay/detail/__tests__/eventProvenance.test.ts`

- [ ] **Step 4: InsightTab 헤더에 배지 + correlation 칩** — nodeHeader에 `eventProvenance(event.kind)` 배지(파랑 native / 보라 derived)와 correlation 칩(tool_use_id·request_id·turn_id 있을 때 표시; 클릭 점프는 이 plan 범위 밖, 표시만). css에 `.badgeNative`/`.badgeDerived`/`.corrChip`.

- [ ] **Step 5: build + commit**
```bash
cd webui && npm run build
git add webui/src/components/replay/detail/eventProvenance.ts webui/src/components/replay/detail/__tests__/eventProvenance.test.ts webui/src/components/replay/detail/InsightTab.tsx webui/src/components/replay/detail/InsightTab.module.css
git commit -m "feat(webui): detail header provenance badge + correlation chips"
```

---

## Task 2: WhatSection 컴포넌트 (타입별 WHAT 본문)

**Files:** `WhatSection.tsx`(+css), `WhatSection.test.tsx`

타입별 "한 일"을 휴먼리더블하게. `matchedResult`(tool_call의 짝 tool_result event)를 받아 결과 출력도 표시.

- [ ] **Step 1: 실패 테스트** `WhatSection.test.tsx`
```typescript
import { render, screen } from '@testing-library/react';
import { WhatSection } from '../WhatSection';

test('tool_call shows command and matched result output', () => {
  const ev = { kind: 'tool_call', payload: { tool_name: 'Bash', input: { command: 'cargo test' } } } as any;
  const result = { payload: { tool_result: { content: 'test result: ok. 142 passed', is_error: false } } } as any;
  render(<WhatSection event={ev} matchedResult={result} />);
  expect(screen.getByText(/cargo test/)).toBeInTheDocument();
  expect(screen.getByText(/142 passed/)).toBeInTheDocument();
});

test('user_message shows full prompt', () => {
  const ev = { kind: 'user_message', payload: { content: '전체 프롬프트 본문' } } as any;
  render(<WhatSection event={ev} matchedResult={null} />);
  expect(screen.getByText(/전체 프롬프트 본문/)).toBeInTheDocument();
});
```

- [ ] **Step 2: 구현** `WhatSection.tsx` — kind별 분기:
  - `tool_call`: `payload.input.command` (Bash) 또는 input 요약(파일경로/패턴 등, nodeLabel.toolArg 로직 참조하되 **전문 표시**) + matchedResult의 `tool_result.content`(앞부분, 길면 truncate) + `is_error` 표시.
  - `user_message`: `payload.content`/`payload.text` 전문.
  - `assistant_message`: `payload.text` 전문.
  - `thinking`: "추론 본문은 기록되지 않음(signature only)" 안내.
  - `hook_event`: hook 이름 + command + stdout/stderr(payload에서).
  - `diff_hunk`: `payload.patch_preview` (@@diff) + file_path.
  - `verification_run`: command + status + failure_summary.
  - 그 외: payload 핵심 필드 요약 또는 "원본은 Raw 탭 참조".
  - monospace 블록, 길면 max-height + scroll. css `.what`/`.cmd`/`.out`/`.err`.

- [ ] **Step 3: 테스트 통과** `npx vitest run src/components/replay/detail/__tests__/WhatSection.test.tsx`

- [ ] **Step 4: commit**
```bash
git add webui/src/components/replay/detail/WhatSection.tsx webui/src/components/replay/detail/WhatSection.module.css webui/src/components/replay/detail/__tests__/WhatSection.test.tsx
git commit -m "feat(webui): WhatSection — per-type 'what it did' body"
```

---

## Task 3: matched tool_result 데이터 흐름

**Files:** `SessionDetailPage.tsx`, `DetailPanel.tsx`

- [ ] **Step 1: SessionDetailPage** — selected event가 `tool_call`이고 `tool_use_id`가 있으면, 현재 로드된 events(stream/window)에서 `kind==='tool_result' && tool_use_id===sel.tool_use_id`인 event를 찾아 `matchedResult`로. (기존 `selectedToolMetrics`가 tool_use_id로 result를 찾는 패턴이 있으니 동일 소스 재사용 — 그 result event 자체를 노출.) `<DetailPanel ... matchedResult={matchedResult} />`.

- [ ] **Step 2: DetailPanel** — `matchedResult?: ObservedEventDto | null` prop 추가, InsightTab에 전달.

- [ ] **Step 3: build** `cd webui && npm run build` (타입 통과). 

- [ ] **Step 4: commit**
```bash
git add webui/src/routes/SessionDetailPage.tsx webui/src/components/replay/detail/DetailPanel.tsx
git commit -m "feat(webui): thread matched tool_result into detail panel"
```

---

## Task 4: InsightTab 5층 통합

**Files:** `InsightTab.tsx`(+css), `InsightTab.test.tsx`

- [ ] **Step 1: 테스트 갱신** — InsightTab.test.tsx: tool_call event + matchedResult 주면 (H)배지·(①)WHAT command·(②)metrics·(③)signals가 순서대로 렌더되는지. user_message면 WHAT에 prompt.

- [ ] **Step 2: InsightTab 재구성** — props에 `matchedResult` 추가. 본문 순서:
  1. **H**: 기존 nodeHeader + provenance 배지(Task1) + correlation 칩.
  2. **① WHAT**: `<WhatSection event={event} matchedResult={matchedResult} />` (event 있을 때). 섹션 타이틀 "What — 한 일".
  3. **② HOW**: `<EntityMetricsPanel ... />` (기존). 섹션 타이틀 "How — 지표".
  4. **③ SIGNALS**: `<SignalsList signals={signals} />` (signals.length>0). 섹션 타이틀 "Signals".
  (RAW는 DetailPanel의 별도 탭 — 유지, 변경 없음.)
  - 헤더의 기존 `label.secondary`(tool 요약 한 줄)는 WHAT이 대체하므로 제거하거나 축약.

- [ ] **Step 3: css** — 섹션 구분(`.section`/`.sectionTitle` 재사용), WHAT을 최상단 강조.

- [ ] **Step 4: build + full vitest** `cd webui && npm run build && npm test` → 0 fail.

- [ ] **Step 5: commit**
```bash
git add webui/src/components/replay/detail/InsightTab.tsx webui/src/components/replay/detail/InsightTab.module.css webui/src/components/replay/detail/__tests__/InsightTab.test.tsx
git commit -m "feat(webui): InsightTab 5-layer skeleton (WHAT lifted to top)"
```

---

## Task 5: 빌드 + 사용자 브라우저 시각 확인

- [ ] **Step 1: full check** `cd webui && npm run build && npm test` → 0 fail. (controller가 dist 임베드 `cargo build`.)

- [ ] **Step 2: 사용자 시각 확인 (controller + 사용자)** — controller가 read-only serve를 띄우고(별도 포트), **사용자가 직접** 디테일 뷰를 열어 확인: tool_call 이벤트 선택 시 ① WHAT에 command + 결과 출력이 보이고, 헤더에 원본/가공 배지가 뜨며, 레이아웃이 깨지지 않는지. user_message 선택 시 프롬프트 전문. (메모리: WebUI 디자인은 사용자가 live로 lock.) 자동화 navigate는 사용자가 거부했으므로 controller는 navigate를 강행하지 않고 URL을 안내한다.

- [ ] **Step 3: 사용자 피드백 반영** — 디자인 조정 요청 시 반영 후 재확인. 승인되면 Plan 2b 완료.

---

## Self-Review 메모
- WHAT의 tool_result 출력은 redaction된 payload(서버에서 이미 redaction) 기준. 길면 truncate + "Raw 탭에서 전문" 안내.
- thinking 본문 미기록은 의도된 한계(메모리: thinking-content-not-recorded) — WHAT에서 명시.
- correlation 칩의 클릭 점프(이벤트 간 이동)는 이 plan 범위 밖(표시만). 점프는 후속.
- 원본/가공 배지: signal은 디테일 뷰에 직접 안 뜨지만(SIGNALS 섹션 안), verification/diff가 이벤트로 선택될 수 있어 derived 처리.
