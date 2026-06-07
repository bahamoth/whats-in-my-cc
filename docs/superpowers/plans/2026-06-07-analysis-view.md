# Analysis View (Plan 3b) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`).

**Goal:** `behavioral_metrics`(Plan 3a `GET /v1/sessions/:id/metrics`)를 보여주는 **별도 "분석" 표면**. replay(디테일 뷰)와 분리(spec §8.3). MVP: 세션 전체 지표(도구 실패율·검증 통과율·캐시 히트율·context_bloat·detector 신호분포). 구간(range) 선택은 백엔드가 세션 전체만 지원하므로 **후속**.

**Architecture:** `getSessionMetrics`/`useSessionMetricsQuery`(GET /metrics). `AnalysisPanel` 컴포넌트가 지표를 카드/표 + detector 분포 막대로 렌더. 세션 상세 페이지에 **"분석" 탭/진입점** 추가(replay와 시각적으로 분리). 결정적 사실 표시 — 임계값 색칠/판단 없음(spec §6.3, §8.3).

**Tech Stack:** TypeScript, React 18, react-query, Vitest.

**Spec:** §8.3(온디맨드 구간 분석 뷰)·§6.6(신호분포). **두 표면 분리(원칙 7):** 이 뷰에 replay 요소를 섞지 않는다.

**Backend contract (Plan 3a):** `GET /v1/sessions/:id/metrics` → `{ data: SessionMetrics }`, `SessionMetrics = { session_id, tool_call_total, tool_failure_count, tool_failure_rate, verification_total, verification_passed, verification_pass_rate, context_bloat_count, cache_hit_ratio, detector_firing: Record<string,number> }`.

---

## File Structure
- Modify: `webui/src/api/types.ts` — `SessionMetricsDto`
- Modify: `webui/src/api/client.ts` — `getSessionMetrics`
- Modify: `webui/src/lib/queries.ts` — `metrics` key + `useSessionMetricsQuery`
- Create: `webui/src/components/replay/analysis/AnalysisPanel.tsx` (+ `.module.css`)
- Modify: 세션 상세 페이지(`SessionDetailPage.tsx` 또는 레이아웃) — "분석" 진입점(탭/패널 토글)
- Test: `AnalysisPanel.test.tsx`, client/queries 테스트 추가

---

## Task 1: types + client + query

**Files:** `types.ts`, `client.ts`, `queries.ts`, client 테스트

- [ ] **Step 1: types.ts** — `SessionMetricsDto`:
```typescript
export type SessionMetricsDto = {
  session_id: string;
  tool_call_total: number;
  tool_failure_count: number;
  tool_failure_rate: number;
  verification_total: number;
  verification_passed: number;
  verification_pass_rate: number;
  context_bloat_count: number;
  cache_hit_ratio: number;
  detector_firing: Record<string, number>;
};
```

- [ ] **Step 2: client.ts** — `export const getSessionMetrics = (id: string): Promise<SessionMetricsDto> => jsonGet<SessionMetricsDto>('/v1/sessions/'+encodeURIComponent(id)+'/metrics');`

- [ ] **Step 3: queries.ts** — `metrics: (id)=>['session',id,'metrics']` key + `useSessionMetricsQuery(id)` (enabled !!id).

- [ ] **Step 4: client 테스트** — `getSessionMetrics`가 `/v1/sessions/:id/metrics` 호출 + `.data` unwrap. (기존 client.endpoints.test.ts 패턴, red first.)

- [ ] **Step 5: build + commit** `cd webui && npm run build`
```bash
git add webui/src/api/types.ts webui/src/api/client.ts webui/src/lib/queries.ts webui/src/api/__tests__/client.endpoints.test.ts
git commit -m "feat(webui): session metrics client + query"
```

---

## Task 2: AnalysisPanel 컴포넌트

**Files:** `AnalysisPanel.tsx`(+css), `AnalysisPanel.test.tsx`

- [ ] **Step 1: 실패 테스트** `AnalysisPanel.test.tsx`
```typescript
import { render, screen } from '@testing-library/react';
import { AnalysisPanel } from '../AnalysisPanel';
const m = { session_id:'s1', tool_call_total:10, tool_failure_count:2, tool_failure_rate:0.2,
  verification_total:4, verification_passed:3, verification_pass_rate:0.75,
  context_bloat_count:1, cache_hit_ratio:0.6, detector_firing:{ tool_failure:2, context_bloat:1 } } as any;
test('renders rates and detector distribution', () => {
  render(<AnalysisPanel metrics={m} />);
  expect(screen.getByText(/20%/)).toBeInTheDocument();      // tool_failure_rate
  expect(screen.getByText(/75%/)).toBeInTheDocument();      // verification_pass_rate
  expect(screen.getByText(/tool_failure/)).toBeInTheDocument(); // detector dist
});
test('empty state when null', () => {
  render(<AnalysisPanel metrics={null} />);
  expect(screen.getByText(/분석할 지표가 없|no metrics/i)).toBeInTheDocument();
});
```

- [ ] **Step 2: 구현** `AnalysisPanel.tsx` — props `{ metrics: SessionMetricsDto | null }`. 렌더:
  - 비율(rate)은 `Math.round(x*100)+'%'`. 카운트는 그대로.
  - 지표 행: 도구 실패율(`tool_failure_count`/`tool_call_total`), 검증 통과율(`verification_passed`/`verification_total`), 캐시 히트율, context_bloat 수.
  - **detector 신호분포**: `detector_firing` 맵을 막대(count 비례 width) 리스트로.
  - 판단/색경고 없음 — 사실만(원칙 6.3). 비율은 회색/중립. `null`이면 빈 상태.
  - css: 카드/표 + 막대(`.bar`). replay와 다른 레이아웃.

- [ ] **Step 3: 테스트 통과** `npx vitest run src/components/replay/analysis/__tests__/AnalysisPanel.test.tsx`

- [ ] **Step 4: commit**
```bash
git add webui/src/components/replay/analysis/
git commit -m "feat(webui): AnalysisPanel — session behavioral metrics (deterministic, no judgment)"
```

---

## Task 3: 진입점 (분석 표면)

**Files:** `SessionDetailPage.tsx`(또는 레이아웃 shell), 테스트

- [ ] **Step 1: 진입점** — 세션 페이지에 "분석" 토글/탭 추가. 클릭 시 `useSessionMetricsQuery(sessionId)` 로 `AnalysisPanel`을 보이는 영역(예: DetailPanel 옆 또는 별도 패널/오버레이)에 렌더. **replay 타임라인과 분리**(원칙 7) — 같은 디테일 패널에 섞지 말고 별도 토글 영역. 기존 레이아웃 패턴(탭/패널)을 따른다.
  - 가장 단순한 MVP: 세션 헤더/메타 영역에 "분석" 버튼 → 클릭 시 AnalysisPanel을 모달/사이드 패널로. 기존 컴포넌트 구조에 맞게 implementer가 자연스러운 위치 선택.

- [ ] **Step 2: 통합 테스트** — SessionDetailPage.test.tsx: `/metrics` mock 추가, 분석 토글 클릭 시 AnalysisPanel 지표가 보이는지. (기존 fetch mock 패턴에 `/metrics` 라우트 추가.)

- [ ] **Step 3: build + full vitest** `cd webui && npm run build && npm test` → 0 fail.

- [ ] **Step 4: commit**
```bash
git add webui/src/routes/SessionDetailPage.tsx webui/src/routes/__tests__/SessionDetailPage.test.tsx
git commit -m "feat(webui): analysis surface entry point (separate from replay)"
```

---

## Task 4: 검증 (코드 스모크는 controller가 직접)
- [ ] **Step 1**: `cd webui && npm run build && npm test` → 0 fail. grep 잔여 없음.
- [ ] **Step 2 (controller)**: 브라우저 시각 확인은 **PR 후 사용자**(claude-in-chrome navigate가 환경에서 거부됨). controller는 cargo build로 dist 임베드 + 통합 테스트로 게이트.

---

## Self-Review 메모
- 구간(range) 선택 UX는 백엔드가 세션 전체만 지원하므로 이 plan 범위 밖 — 후속(백엔드 range 파라미터 추가 시).
- AnalysisPanel은 **판단/임계값 색칠 없음** — 비율·카운트 사실만. "나쁜가"는 보는 사람(LLM/사람)이.
- 두 표면 분리(원칙 7): 분석 패널을 replay 디테일(InsightTab)에 섞지 않는다. 별도 토글/패널.
- detector_firing 막대는 신호분포(§6.6)의 1차 시각화 — 발화율(이벤트 대비)은 백엔드 정교화 후.
