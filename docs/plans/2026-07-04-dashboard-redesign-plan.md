# 대시보드 전면 개편 구현 계획

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `docs/specs/2026-07-04-dashboard-redesign.md`의 확정 스펙(승인 목업 `docs/mockups/dash-full-mockup.html`, `dash-verification-mockup.html`)대로 대시보드를 개요/검증 2탭으로 재작성하고 /sessions 리스트에 지표 컬럼을 추가한다.

**Architecture:** 프론트는 ECharts(tree-shaken core) 래퍼 + 순수 파생 함수(`lib/dashDerive.ts`, vitest 잠금) + 모듈별 컴포넌트(`components/dash/*`)로 분해. 검증 탭 집계는 신규 백엔드 `GET /v1/verification/summary`(read-only, TDD)가 공급. 이전-창 delta는 프론트가 `/v1/metrics`를 이전 동일 창으로 한 번 더 호출해 계산.

**Tech Stack:** React 18 + TS, ECharts 5(core+Bar/Scatter/Line/Sankey), 기존 axum+sqlx 백엔드, vitest / cargo test.

## Global Constraints (스펙 §0 — 모든 태스크에 적용)

- 판정 문장·질문형 제목 금지. 모든 문구는 i18n 카탈로그(en SSOT) 경유.
- 모델명은 전체 표시명 — `displayModel()`만 사용, `shortModel` UI 사용처 제거.
- 미측정 ≠ 0: 값 없으면 `—` 또는 셀 미표시. 코호트 비교는 전/후 n 병기, n<3이면 delta 강조 해제 + "표본 부족".
- 좌표계 있는 시각화 = ECharts, 텍스트 배치(카드 레인·커버리지 바) = DOM.
- 색: 통과 `#41c285` 실패 `#ef4747` 판정불가 `#4a5162` 미실행 `#3d4351`, 신호 램프 `#41c285→#c9c04a→#f0a03c→#ef6047`, 모델 슬롯 = 기존 `MODEL_SLOTS`.
- 커밋마다: 실패 테스트 먼저(빨강 확인) → 구현 → green → commit. UI 태스크는 브라우저 스모크 후 commit.
- 스펙 대비 세부 확정 2건: ① 가드 실행 리듬의 진행률은 **시간 기준** `pct = (run.started_at − session.first)/(session.last − session.first)` (이벤트 서수 조인보다 단순·결정론 동일). ② `not_executed`(실데이터 45건)는 판정불가에 합치지 않고 **별도 범주 "미실행"** 으로 kind 스택·Sankey에 표기.

---

### Task 1: ECharts 도입 + `<EChart>` 래퍼

**Files:**
- Modify: `webui/package.json` (dep `echarts@^5.5`)
- Create: `webui/src/components/dash/EChart.tsx`
- Create: `webui/src/components/dash/echartsBase.ts`
- Test: `webui/src/components/dash/__tests__/EChart.test.tsx`

**Interfaces:**
- Produces: `EChart({option, height, onEvents?, group?}): JSX` — option 변경 시 `setOption(option,{notMerge:true})`, unmount 시 dispose, ResizeObserver로 resize, `group` 지정 시 `echarts.connect(group)`.
- Produces: `echartsBase.TOOLTIP`, `echartsBase.AXIS`(라벨/스플릿 스타일), `echartsBase.rampColor(t:number):string` — 목업의 TT/AX/RAMP를 그대로 상수화.

- [ ] **Step 1: 의존성 추가** — `cd webui && npm i echarts@^5.5`
- [ ] **Step 2: 실패 테스트 작성** — `EChart.test.tsx`:

```tsx
import { render, cleanup } from '@testing-library/react';
import { describe, it, expect, vi, afterEach } from 'vitest';
const init = vi.fn(() => ({ setOption: vi.fn(), resize: vi.fn(), dispose: vi.fn(), on: vi.fn() }));
vi.mock('echarts/core', async (orig) => ({ ...(await orig()), init }));
import { EChart } from '../EChart';

afterEach(cleanup);
describe('EChart', () => {
  it('mounts → echarts.init, unmount → dispose', () => {
    const { unmount } = render(<EChart option={{ series: [] }} height={100} />);
    expect(init).toHaveBeenCalledTimes(1);
    const inst = init.mock.results[0].value;
    unmount();
    expect(inst.dispose).toHaveBeenCalled();
  });
  it('option 변경 시 setOption(notMerge)', () => {
    const { rerender } = render(<EChart option={{ series: [] }} height={100} />);
    const inst = init.mock.results.at(-1)!.value;
    rerender(<EChart option={{ series: [{}] }} height={100} />);
    expect(inst.setOption).toHaveBeenLastCalledWith({ series: [{}] }, { notMerge: true });
  });
});
```

- [ ] **Step 3: 빨강 확인** — `npx vitest run src/components/dash` → FAIL (모듈 없음)
- [ ] **Step 4: 구현** — `EChart.tsx`:

```tsx
import { useEffect, useRef } from 'react';
import * as echarts from 'echarts/core';
import { BarChart, LineChart, ScatterChart, SankeyChart } from 'echarts/charts';
import {
  GridComponent, TooltipComponent, DataZoomComponent,
  MarkLineComponent, LegendComponent,
} from 'echarts/components';
import { CanvasRenderer } from 'echarts/renderers';

echarts.use([BarChart, LineChart, ScatterChart, SankeyChart, GridComponent,
  TooltipComponent, DataZoomComponent, MarkLineComponent, LegendComponent, CanvasRenderer]);

export function EChart({ option, height, group, onEvents }: {
  option: object; height: number; group?: string;
  onEvents?: Record<string, (p: unknown) => void>;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const chart = useRef<echarts.ECharts>();
  useEffect(() => {
    const c = echarts.init(ref.current!);
    chart.current = c;
    if (group) { c.group = group; echarts.connect(group); }
    for (const [ev, fn] of Object.entries(onEvents ?? {})) c.on(ev, fn);
    const ro = new ResizeObserver(() => c.resize());
    ro.observe(ref.current!);
    return () => { ro.disconnect(); c.dispose(); };
    // onEvents/group은 마운트 시 1회 — 재바인딩 불필요(모듈 정적)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  useEffect(() => { chart.current?.setOption(option, { notMerge: true }); }, [option]);
  return <div ref={ref} style={{ height, width: '100%' }} />;
}
```

`echartsBase.ts` — 목업 상수 이식:

```ts
export const TOOLTIP = {
  backgroundColor: '#1b202a', borderColor: '#2a3040', borderWidth: 1, padding: [9, 12],
  textStyle: { color: '#e6e8ee', fontSize: 12 },
  extraCssText: 'border-radius:10px;box-shadow:0 14px 40px -18px rgba(0,0,0,.9)',
} as const;
export const AXIS_LABEL = {
  color: '#6a7180', fontFamily: 'ui-monospace,Menlo,monospace', fontSize: 10.5,
} as const;
export const SPLIT_LINE = { lineStyle: { color: '#171b24' } } as const;
const RAMP: Array<[number, string]> = [[0, '#41c285'], [0.35, '#c9c04a'], [0.7, '#f0a03c'], [1, '#ef6047']];
export function rampColor(t: number): string { /* 목업 rampColor와 동일한 구간 보간 */
  const cl = Math.max(0, Math.min(1, t));
  const hx = (c: string) => [1, 3, 5].map((i) => parseInt(c.slice(i, i + 2), 16));
  for (let k = 1; k < RAMP.length; k++) {
    if (cl <= RAMP[k][0]) {
      const [t0, c0] = RAMP[k - 1]; const [t1, c1] = RAMP[k];
      const u = (cl - t0) / (t1 - t0); const a = hx(c0); const b = hx(c1);
      return '#' + a.map((v, j) => Math.round(v + (b[j] - v) * u).toString(16).padStart(2, '0')).join('');
    }
  }
  return RAMP[RAMP.length - 1][1];
}
```

- [ ] **Step 5: green 확인 + rampColor 테스트 추가** (`rampColor(0)==='#41c285'`, `rampColor(1)==='#ef6047'`, 중간값 hex 형식) → `npx vitest run src/components/dash` PASS
- [ ] **Step 6: Commit** — `feat(webui): ECharts tree-shaken 래퍼 + 차트 베이스 상수`

### Task 2: 파생 함수 `lib/dashDerive.ts` + `displayModel`

**Files:**
- Modify: `webui/src/lib/seriesView.ts` (displayModel 추가)
- Create: `webui/src/lib/dashDerive.ts`
- Test: `webui/src/lib/__tests__/dashDerive.test.ts`, `seriesView.test.ts`에 displayModel 케이스 추가

**Interfaces (Produces — 이후 태스크가 그대로 사용):**

```ts
// seriesView.ts
displayModel(name: string): string            // 'claude-fable-5'→'Fable 5', 'haiku-4-5-20251001'→'Haiku 4.5', 미인식은 원문(claude- 제거)
// dashDerive.ts  (rows = SessionSeriesRowDto[], 시간 오름차순 가정)
buildDaily(rows): { dates: string[](MM-DD); cost: number[]; signals: number[];
  passed: number[]; failed: number[]; unknown: number[]; sessionsOf: number[][] /* row index per day */ }
headline(rows): { sessions: number; events: number; passRatePct: number|null;
  cost: number; unitRatePerM: number|null; cacheHitPct: number|null; toolFailPct: number|null }
headlineDelta(cur, prev): { passRatePp: number|null; cost: number|null; unitRate: number|null;
  cacheHitPp: number|null; toolFailPp: number|null }   // prev 없으면 모두 null
observedChanges(rows): string[]  // 결정론: 코호트 diff("MM-DD {displayModel} 유입" 등) + "신호 최다 {slug} (n)"
cohortCompare(rows): null | { label: string; boundaryIdx: number; alsoCcChanged: boolean;
  before: CohortAgg; after: CohortAgg; lowSample: boolean }
  // CohortAgg = { n: number; unitRatePerM: number|null; passRatePct: number|null;
  //               signalsPerSession: number; cacheHitPct: number|null }
laneLayout(items: Array<{x: number/*0-100*/}>, cardWidthPct: number):
  number[]  // item별 lane index — 왼쪽부터 greedy, 겹치면 다음 레인
```

- [ ] **Step 1: 실패 테스트 작성** — 핵심 케이스(각 함수 2~4개):

```ts
describe('displayModel', () => {
  it('full display names', () => {
    expect(displayModel('claude-fable-5')).toBe('Fable 5');
    expect(displayModel('claude-opus-4-8')).toBe('Opus 4.8');
    expect(displayModel('haiku-4-5-20251001')).toBe('Haiku 4.5');
    expect(displayModel('claude-sonnet-4-6')).toBe('Sonnet 4.6');
    expect(displayModel('weird_model')).toBe('weird_model');
  });
});
describe('cohortCompare', () => {
  it('최신 경계의 인접 세그먼트를 전/후로 집계하고 라벨을 diff에서 파생', () => { /* fern 픽스처: O→O+F 경계 → label 'Fable 5 유입' */ });
  it('경계 없으면 null, 한쪽 n<3이면 lowSample', () => { /* … */ });
  it('같은 경계에서 cc_versions도 달라지면 alsoCcChanged', () => { /* … */ });
});
describe('laneLayout', () => {
  it('겹치지 않으면 lane 0 유지, 겹치면 증가', () => {
    expect(laneLayout([{x:0},{x:5},{x:40}], 20)).toEqual([0,1,0]);
  });
});
describe('buildDaily', () => { it('일자 버킷 합산 + sessionsOf 역참조', () => { /* 2세션 같은 날 → cost 합, sessionsOf[i] 길이 2 */ }); });
describe('headlineDelta', () => { it('prev 없으면 전부 null', () => { /* … */ }); });
```

- [ ] **Step 2: 빨강 확인** → **Step 3: 구현** (모든 비율은 usageRatios 재사용, 반올림 1자리; passRate는 passed+failed 합이 0이면 null) → **Step 4: green** → **Step 5: Commit** `feat(webui): 대시보드 파생 SSOT lib/dashDerive + displayModel`

### Task 3: 백엔드 `GET /v1/verification/summary`

**Files:**
- Modify: `src/api/dto.rs`(응답 DTO), `src/api/routes.rs`(handler), `src/api/mod.rs`(route 등록)
- Test: `tests/api_verification_summary.rs` (기존 `tests/api_*.rs` 픽스처 패턴 — in-memory pool + seed)

**Interfaces (Produces):** 쿼리 `?project=&from=&to=` (metrics_series와 동일 파싱 규칙). 응답 `{ meta, data }`:

```json
{
  "total": 793, "not_executed": 45,
  "by_kind": [{ "kind": "test", "passed": 310, "failed": 55, "unknown": 47, "not_executed": 0 }],
  "status_basis": { "exit": 632, "piped": 118, "other": 43 },
  "failures": { "recovered": 61, "abandoned": 26 },
  "rhythm": [{ "session_id": "…", "guards": 83, "passed": 57,
               "runs": [{ "pct": 4.2, "status": "failed" }] }],
  "coverage": { "covered": 524, "total": 738,
                "by_session": [{ "session_id": "…", "covered": 182, "total": 223 }] }
}
```

결정론 정의(테스트가 잠근다):
- `recovered`: failed run과 같은 (session_id, command_kind)에 `started_at`이 더 늦은 passed run 존재. 아니면 `abandoned`.
- `rhythm`: run 수 상위 4개 세션, `pct = (started_at − session.first_observed_at)/(last − first) × 100` (span 0 → 50.0), 소수 1자리.
- `coverage`: 세션별 diff_hunk 총수 대비, **passed run의 covered_diff_hunk 집합 합집합** 크기 — 기존 `routes.rs`의 temporal-precedence 커버 계산 함수를 재사용(refactor해서 공유, 동작 변경 금지). by_session은 hunk 총수 상위 6개.
- `status_basis.other` = piped/exit 외 전부(현재 스키마상 hook·otel 유래 unknown 포함).

- [ ] **Step 1: 실패 테스트** — 시드: 세션 2개(A: test failed→passed 순서로 2 run + build passed, hunk 3개 중 2 커버; B: lint unknown(status_basis piped) 1 run, hunk 1개 커버 0). 단언: by_kind 합계, failures {recovered:1, abandoned:0}, status_basis.piped==1, rhythm pct 순서 보존, coverage {covered:2,total:4}.
- [ ] **Step 2: 빨강 확인** — `cargo test --test api_verification_summary` FAIL(404)
- [ ] **Step 3: 구현** — handler는 metrics_series의 시간 파싱·프로젝트 필터를 그대로 복제, 집계는 SQL(뼈대):

```sql
-- by_kind / status_basis / not_executed
SELECT command_kind, status, status_basis, COUNT(*) FROM verification_run vr
JOIN session s ON s.session_id = vr.session_id
WHERE (?1 IS NULL OR s.project = ?1)
  AND (?2 IS NULL OR vr.started_at >= ?2) AND (?3 IS NULL OR vr.started_at <= ?3)
GROUP BY 1,2,3;
-- failures: run별 후행 passed 존재 여부
SELECT vr.verification_run_id,
  EXISTS(SELECT 1 FROM verification_run p WHERE p.session_id=vr.session_id
         AND p.command_kind=vr.command_kind AND p.status='passed'
         AND p.started_at > vr.started_at) AS recovered
FROM verification_run vr WHERE vr.status='failed' /* + 동일 필터 */;
```

kind 매핑: `test_suite_* → test`, `build|build_check → build`, `lint → lint`, `format_check → format` (그 외 원문 유지 — typecheck kind가 스키마에 없으면 표기하지 않는다. **스키마에 없는 범주를 지어내지 않는다**).
- [ ] **Step 4: green + `cargo fmt` + `clippy -D warnings`** → **Step 5: TS 타입/클라이언트** — `webui/src/api/types.ts`에 `VerificationSummaryDto`, `client.ts`에 `getVerificationSummary(opts)` + `client.endpoints.test.ts` 케이스 1개 → vitest green → **Step 6: Commit** `feat(api+webui): 검증 집계 요약 endpoint /v1/verification/summary (TDD)`

### Task 4: DashboardPage 골격 — 탭·데이터 훅·헤드라인

**Files:**
- Modify: `webui/src/routes/DashboardPage.tsx` (전면 재작성 — 기존 Recharts 모듈 제거)
- Create: `webui/src/components/dash/HeadlineStats.tsx`
- Modify: `webui/src/i18n/catalog/en.ts`, `ko.ts` (dash.* 키 전면 교체 — 서술형 라벨, 스펙 원칙 3)
- Test: `webui/src/routes/__tests__/DashboardPage.test.tsx` 재작성

**Interfaces:**
- Consumes: Task 2 `headline/headlineDelta/observedChanges`, Task 1 `EChart`.
- Produces: `DashboardPage` 내부 상태 `{ rows, prevRows, tab }` — 이전 창 fetch는 `windowKey!=='all'`일 때만(`from−span → from`), 실패는 delta 전부 null(경고 없음, fnote '비교 없음').

- [ ] **Step 1: 실패 테스트** — 기존 파일 교체:

```tsx
it('헤드라인 stat 5개와 delta 칩을 렌더', async () => { /* mockFetch 2창 → '검증 통과' '추정 비용' '블렌디드 단가' '캐시 적중' '도구 실패율' + '▲' 존재 */ });
it("windowKey==='all'이면 delta 없이 fnote '비교 없음'", async () => { /* … */ });
it('개요/검증 탭 전환 — 검증 탭은 summary fetch 후 제목 렌더', async () => { /* userEvent.click, /v1/verification/summary mock */ });
it('API 실패 시 role=alert', async () => { /* 기존 케이스 이식 */ });
```

- [ ] **Step 2: 빨강** → **Step 3: 구현** — 페이지 구조: 상단 타이틀+창 토글(기존 유지), `Tabs`(개요|검증). `HeadlineStats` props `{ h: Headline; d: HeadlineDelta|null }` — 목업 `.stats` 마크업을 Tailwind로 이식(5칸 grid, mono 숫자, delta 칩 good/bad/flat + fnote). "관측된 변화" 줄은 `observedChanges(rows).join(' · ')`.
- [ ] **Step 4: green + tsc** → **Step 5: Commit** `feat(webui): 대시보드 골격 재작성 — 개요/검증 탭 + 문자 헤드라인`

### Task 5: 개요 탭 모듈 — 일별 검증 / 일별 비용·신호

**Files:**
- Create: `webui/src/components/dash/DailyVerification.tsx`, `DailyCostSignals.tsx`
- Test: `webui/src/components/dash/__tests__/dailyCharts.test.tsx` (option 생성 함수 단위 검증)

**Interfaces:**
- Consumes: `buildDaily`, `cohortCompare`(markLine 위치), `EChart`, `rampColor`.
- Produces: `buildVerOption(daily, markers): EChartsOption`, `buildCostOption(daily, markers): EChartsOption` — **순수 함수로 분리 export** (렌더는 `<EChart option={…} group="dash-t">`). markers = `[{ dayIdx, label }]`.

- [ ] **Step 1: 실패 테스트** — option 함수 단언: 스택 series 3개+색, markLine data에 라벨('Fable 5 유입' — displayModel 경유), cost 막대 `itemStyle.color === rampColor(sig/max)`, dataZoom slider는 cost에만.
- [ ] **Step 2: 빨강** → **Step 3: 구현** — 목업 `chVer`/`chTime` option을 TS로 이식(툴팁 formatter의 그날 세션 목록은 `daily.sessionsOf[i]` → slug·값. 캡션: 검증 = "가드 n · 통과 m" 배지 + "가드 0 세션 k개", 비용 = 그라데이션 범례 스와치(0→max) — HTML 캡션은 컴포넌트가 렌더).
- [ ] **Step 4: DashboardPage 개요 탭에 장착 + vitest green** → **Step 5: 브라우저 스모크**(스크래치 serve+vite, 두 차트 crosshair 동기·dataZoom 확인) → **Step 6: Commit** `feat(webui): 일별 검증·비용/신호 모듈 (ECharts, 신호 그라데이션)`

### Task 6: 코호트 비교 슬로프 카드

**Files:**
- Create: `webui/src/components/dash/CohortCompare.tsx`
- Test: `webui/src/components/dash/__tests__/CohortCompare.test.tsx`

**Interfaces:** Consumes `cohortCompare(rows)`. 렌더: 카드 4장(단가·통과율·신호/세션·캐시 적중) — 큰 후값+delta 칩+`전 → 후` fnote+2점 슬로프(`EChart` line, 나쁜 방향 amber/좋은 방향 green — 방향 정의: 단가·신호/세션은 증가=amber, 통과율·적중은 감소=amber). 제목 `t('dash.cohort.title', label)` = "코호트 비교 — {label} 전후". `lowSample`이면 delta 칩 대신 `표본 부족(전 n·후 m)` 배지. `alsoCcChanged`면 fnote 각주. `cohortCompare===null`이면 섹션 미렌더.

- [ ] **Step 1: 실패 테스트** — 라벨 렌더('Fable 5 유입'), lowSample 배지, null → 미렌더.
- [ ] **Step 2: 빨강** → **Step 3: 구현** → **Step 4: green** → **Step 5: Commit** `feat(webui): 코호트 비교 슬로프 — 최신 경계 결정론 라벨`

### Task 7: 세션 타임라인 카드 레인

**Files:**
- Create: `webui/src/components/dash/SessionCardLane.tsx`
- Test: `webui/src/components/dash/__tests__/SessionCardLane.test.tsx`

**Interfaces:** Consumes `laneLayout`, `displayModel`, `usageRatios`. 카드 = 목업 `.scard` 이식: 슬러그(마지막 `-` 세그먼트 아님 — **전체 슬러그를 12자 ellipsis**, title=full), 모델 전체명(색), `$비용 · n신호 · 통과%`, `날짜 · events`. 신호 밀도(신호/100ev)가 창 중앙값×2 초과 && 신호>2 → 신호 숫자 `text-(--wimcc-warn)`. 클릭 → `navigate('/sessions/'+sid)`. x = `dayIdx/(spanDays-1)`, 우측 클램프. 레인 스택. usage 미측정 카드: `— · 0신호 · —` 대신 `모델 미관측`/`—` 표기(0 위장 금지).

- [ ] **Step 1: 실패 테스트** — 카드 수 = rows 수, 겹치는 두 세션 top 상이(laneLayout), 클릭 navigate 호출, 미측정 세션 `—` 렌더.
- [ ] **Step 2: 빨강** → **Step 3: 구현** → **Step 4: green + 브라우저 스모크** → **Step 5: Commit** `feat(webui): 세션 타임라인 카드 레인 — 시간축 귀속·리듬`

### Task 8: 세션 분포 스캐터

**Files:**
- Create: `webui/src/components/dash/SessionScatter.tsx`
- Test: `webui/src/components/dash/__tests__/SessionScatter.test.tsx` (option 순수 함수)

**Interfaces:** Produces `buildScatterOption(rows): {option, medX, medY}` — x 과금 토큰 M(log, 0 제외), y 신호/100ev, size `6+√cost×1.15`, series = 주 모델별(색 MODEL_SLOTS 순서 고정 — **빈도순이 아니라 최초 관측순**, 색이 창 변경에 흔들리지 않게), markLine 중앙값 점선, 이상점 라벨(비용 상위 2 ∪ y 상위 2). onEvents click → navigate. 캡션 서술형: "세션 분포 — 과금 토큰 × 신호 밀도".

- [ ] **Step 1: 실패 테스트** — 토큰 0 세션 제외, series 그룹 수 = 주 모델 종수, 라벨 formatter가 이상점만 비공백.
- [ ] **Step 2: 빨강** → **Step 3: 구현(목업 chSc 이식)** → **Step 4: green + 스모크** → **Step 5: Commit** `feat(webui): 세션 분포 스캐터 (log·중앙값 사분면·이상점 라벨)`

### Task 9: 검증 탭

**Files:**
- Create: `webui/src/components/dash/VerificationTab.tsx` (헤드라인 5칸 + 하위 4모듈 조립)
- Create: `webui/src/components/dash/verificationOptions.ts` (`buildKindOption`, `buildSankeyOption` 순수 함수)
- Create: `webui/src/components/dash/GuardRhythm.tsx`, `ChangeCoverage.tsx` (DOM)
- Test: `webui/src/components/dash/__tests__/verification.test.tsx`

**Interfaces:** Consumes Task 3 `VerificationSummaryDto`. 헤드라인: 가드 실행(kind 분해 fnote) / 측정률(판정불가 분해) / 통과(측정분, 이전 창 delta는 **생략** — summary에 prev 없음, fnote 없이 숫자만) / 실패 방치 / 변경 커버리지(미커버 hunk 배지). Sankey 노드·링크 = 목업 구조 + `미실행` 노드 추가(`가드→미실행 n`). 리듬 = summary.rhythm 4세션 스트립(목업 `.rh-*` 이식). 커버리지 = by_session 바(커버 green/미커버 amber, `0%` 강조).

- [ ] **Step 1: 실패 테스트** — buildSankeyOption 링크 보존(합계 일치·미실행 포함), buildKindOption 100% 정규화, GuardRhythm dot 수 = runs 수, 커버리지 % 계산.
- [ ] **Step 2: 빨강** → **Step 3: 구현** → **Step 4: DashboardPage 검증 탭 장착(탭 진입 시 lazy fetch, 로딩 Skeleton) + green + 스모크** → **Step 5: Commit** `feat(webui): 검증 탭 — 측정률·행방 Sankey·실행 리듬·변경 커버리지`

### Task 10: /sessions 리스트 지표 컬럼 추가

**Files:**
- Modify: `webui/src/routes/SessionListPage.tsx` (+ module.css 필요 시)
- Test: `webui/src/routes/__tests__/SessionListPage.test.tsx` 확장

**Interfaces:** Consumes `getMetricsSeries({limit:200})` — session_id로 join(리스트의 기존 fetch에 병행, 실패해도 기존 컬럼은 렌더). **기존 컬럼 전부 유지** + 추가: 검증(`통과/전체` + pass/fail 마이크로바) · 신호 · 비용($) · 단가($/1M, `usageRatios`) · 적중(%). 헤더 클릭 정렬(기존 last-seen 정렬과 통합, 방향 토글). 미측정 `—`.

- [ ] **Step 1: 실패 테스트** — metrics mock 주입 시 컬럼 값 렌더, metrics fetch 실패 시 기존 컬럼 정상 + `—`, 비용 헤더 클릭 → 정렬 순서 변경.
- [ ] **Step 2: 빨강** → **Step 3: 구현** → **Step 4: green + 스모크** → **Step 5: Commit** `feat(webui): 세션 리스트 지표 컬럼 추가(기존 컬럼 유지) + 정렬`

### Task 11: 마감 — 전체 검증·개선 루프·notes

- [ ] **Step 1:** `npx tsc --noEmit && npx vitest run` / `cargo fmt --check && cargo clippy -- -D warnings && cargo test` 전부 green (cargo test는 백그라운드 + 로그 grep으로 실패 0 증명)
- [ ] **Step 2:** 사용 안 하게 된 코드 제거 — Recharts 대시보드 잔재(chart config·Brush 관련 미사용 import), `shortModel` UI 사용처 0 확인(`grep -rn "shortModel(" webui/src --include="*.tsx"` → seriesView 내부·테스트만), 미사용 i18n 키 제거(en/ko 동기)
- [ ] **Step 3:** 브라우저 최종 스모크 — 스크래치 serve(:7999)+Vite(:5174): 두 탭 전 모듈, crosshair 동기, 카드/스캐터 클릭 → 리플레이, /sessions 정렬. 스크린샷 캡처해 사용자 보고에 첨부
- [ ] **Step 4:** 개선 루프 4종(`untagged-bash`·`unknown-verification`·`unidentified-plugins`·`tagging-gate` — 새 바이너리 기준) 실행, 게이트 pass 확인
- [ ] **Step 5:** `docs/implementation-notes.html`에 `#dashboard-redesign-2026-07-04` 섹션(ECharts 도입 근거·번들 영향 측정치·스펙 편차 2건) 추가
- [ ] **Step 6:** Commit + push (PR #91 갱신 — **병합 금지**, CI green 확인까지만)

## Self-Review 결과

- 스펙 커버리지: §1 모듈1~6 → Task 4~8 / §2 → Task 2·6 / §3 → Task 3·9 / §4 → Task 10 / §5 ECharts → Task 1(번들 측정은 Task 11 notes) / §6 탭 배치 → Task 4. 공백 없음.
- typecheck kind: 스키마(`command_kind`)에 존재하지 않아 **표기하지 않기로 확정**(Global Constraints의 "지어내지 않는다"와 스펙 원칙 7 일치 — 목업의 typecheck 행은 목업 한정).
- 타입 일관성: `cohortCompare` 반환을 Task 4·5·6이 공유, option 빌더는 전부 순수 함수 export로 통일.
