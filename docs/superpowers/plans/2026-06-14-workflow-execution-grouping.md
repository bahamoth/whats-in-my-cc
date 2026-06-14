# Workflow 실행 그룹핑·가시화 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** replay 스트림에서 `Workflow` 툴이 띄운 fan-out 서브에이전트들을 turn_id 기준으로 한 "워크플로우 실행" 컨테이너로 묶고, 접힘 상태에서도 종합·요약 통계·미니 간트 차트를 노출한다(승인된 프로토타입 A안 = 하이브리드+미니 간트).

**Architecture:** 순수 webui 변경. `Workflow` tool_call은 main 체인에 1개 + `turn_id`만 남기고 fan-out 에이전트는 사이드체인에만 존재(사이드카·per-agent tool_call 없음). 따라서 BatchGroup의 사이드카 조인과 별개로, **agent의 turn_id ↔ 같은 turn의 Workflow tool_call**로 묶는다(사이드카 불필요 → 라이브에서도 견고). `buildStreamModel`의 flush 단계에서 사이드카로 message_id가 안 잡히는 에이전트를 turn_id의 Workflow call로 라우팅한다. 워크플로우 이름은 tool_call `payload.input.script`의 `meta` 리터럴에서 파싱.

**Tech Stack:** TypeScript, React, vitest, @testing-library/react. 설계 근거: 이 대화의 승인된 브라우저 프로토타입(`webui/public/wf-hybrid.html`, A안) + `docs/superpowers/specs/2026-06-13-subagent-parallel-batch-grouping-design.md`.

**실데이터 앵커(real-data anchored):** `witmcc-ux-investigate`(turn 9112ef42, 5 agents 순수 병렬, 2~38m), `insight-redesign-evidence`(turn be6dca26, 9 agents 4-stage pipeline). 전 DB 검증: Workflow 에이전트 69개(653ea169:43·89f99c17:26)는 사이드카 0·Agent tool_call 0·turn_id 공유. 백엔드 DTO에 `turn_id`·`payload.input.script` 이미 노출(실측 확인).

**범위 밖(별도 plan):** ① 라이브 serve가 mid-session 사이드카(meta.json)를 안 먹는 점 — Agent-배치 경로 전용이라 본 plan과 무관(Workflow는 turn_id로 묶음). ② `store.rs` `ingest_sidecar_file` dedup early-return 잠재버그. ③ Workflow stage/배리어 경계의 정밀 추론(본 plan은 시작-시각 기반 근사).

---

## File Structure

- `webui/src/components/replay/stream/streamModel.ts` (modify) — `WorkflowGroup` 타입·union, `parseWorkflowMeta`, Workflow call prepass, agent→workflow 라우팅, synthesis 일반화.
- `webui/src/components/replay/stream/workflowStats.ts` (create) — 순수 헬퍼: `groupSpanMs`, `workflowStats`(maxConcurrency·longest·median·incomplete), `workflowTimeline`(lanes·gaps), `agentDurationHeat`.
- `webui/src/components/replay/stream/WorkflowGroup.tsx` (create) — 컨테이너. 접힘=헤더+종합+통계칩+미니간트(항상), 펼침=간트(키↑)+자식 `SubagentGroup`.
- `webui/src/components/replay/stream/WorkflowGroup.module.css` (create) — orange(`--wimcc-lane-quality`) 레일·간트·칩 (토큰 사용).
- `webui/src/components/replay/stream/ConversationStream.tsx` (modify) — `renderItem`·`itemContainsEvent`에 `workflow-group`.
- `webui/src/components/replay/stream/SubagentGroup.tsx` + `BatchGroup.tsx` (modify) — 각 `itemEventIds`에 `workflow-group` 케이스(자동 펼침 contains).
- Tests: `__tests__/buildStreamModel.test.ts`(modify), `__tests__/workflowStats.test.ts`(create), `__tests__/WorkflowGroup.test.tsx`(create), `__tests__/ConversationStream.test.ts`(modify).

---

## Task 1: 타입 + parseWorkflowMeta

**Files:** Modify `streamModel.ts`, `__tests__/buildStreamModel.test.ts`

- [ ] **Step 1: 실패 테스트** — `buildStreamModel.test.ts` 상단 import 아래 추가:

```ts
import { parseWorkflowMeta } from '../streamModel';

describe('parseWorkflowMeta', () => {
  it('meta 리터럴에서 name·description 추출', () => {
    const s = "export const meta = {\n  name: 'review-changes',\n  description: 'Review the diff',\n  phases: []\n}\nphase('x')";
    expect(parseWorkflowMeta(s)).toEqual({ name: 'review-changes', description: 'Review the diff' });
  });
  it('큰따옴표·없는 필드 처리', () => {
    expect(parseWorkflowMeta('export const meta = { name: "wf-1" }')).toEqual({ name: 'wf-1', description: null });
    expect(parseWorkflowMeta('no meta here')).toEqual({ name: null, description: null });
  });
});
```

- [ ] **Step 2: 실패 확인** — Run: `cd webui && npx vitest run src/components/replay/stream/__tests__/buildStreamModel.test.ts -t parseWorkflowMeta` · Expected: FAIL (`parseWorkflowMeta` 없음).

- [ ] **Step 3: 구현** — `streamModel.ts`에 export 추가(파일 상단 helper 영역):

```ts
/** Workflow tool_call의 `payload.input.script`에 박힌 `meta = {name, description}`
 *  리터럴에서 이름·설명만 끌어낸다. 스크립트 전체 평가가 아니라 표층 정규식 —
 *  meta는 순수 리터럴 규약(작은/큰따옴표만)이라 충분하다. 못 찾으면 null. */
export function parseWorkflowMeta(script: string): { name: string | null; description: string | null } {
  const pick = (key: string): string | null => {
    const m = script.match(new RegExp(key + "\\s*:\\s*['\"]([^'\"]*)['\"]"));
    return m ? m[1] : null;
  };
  return { name: pick('name'), description: pick('description') };
}
```

- [ ] **Step 4: WorkflowGroup 타입 추가** — `BatchGroup` 인터페이스 아래에:

```ts
/** `Workflow` 툴이 띄운 fan-out 서브에이전트 묶음. main 체인엔 Workflow tool_call
 *  1개 + turn_id만 남으므로(사이드카·per-agent tool_call 없음) 같은 turn의
 *  Workflow call로 묶는다. 동시성은 에이전트 사이에만, 각 자식은 직렬. */
export interface WorkflowGroup {
  type: 'workflow-group';
  id: string;
  /** meta.name (없으면 null → 컴포넌트가 '워크플로우'로 표기). */
  name: string | null;
  description: string | null;
  /** 디스패치한 Workflow tool_call의 event_id (점프 타깃). */
  taskEventId: string | null;
  agentGroups: SidechainGroup[];
  /** 실행 종료 후 main의 첫 assistant_message = 종합. null=진행 중/미관측. */
  synthesis: string | null;
  /** 모든 자식이 결론 보유면 true. */
  settled: boolean;
}
```

union 갱신:
```ts
export type StreamItem =
  | MessageItem
  | ActivityRun
  | SidechainGroup
  | ThinkingMarker
  | BatchGroup
  | WorkflowGroup
  | ScaffoldGroup;
```

- [ ] **Step 5: 통과 확인** — Run: `npx vitest run ...buildStreamModel... -t parseWorkflowMeta` · Expected: PASS. `npx tsc --noEmit` 타입 에러 없음.

- [ ] **Step 6: 커밋**
```bash
git add -A && git commit -m "test(webui): workflow 그룹핑 red — WorkflowGroup 타입·parseWorkflowMeta"
```

---

## Task 2: streamModel — turn_id로 Workflow 라우팅

**Files:** Modify `streamModel.ts`, `__tests__/buildStreamModel.test.ts`

기존 테스트 헬퍼(`asstMain/taskCall/sidecar/scUser/scAsst/base`) 재사용. Workflow용 헬퍼 추가가 필요하다.

- [ ] **Step 1: 테스트 헬퍼 추가** (기존 헬퍼 옆):

```ts
const wfCall = (mid: string, ev: string, tu: string, turn: string, name: string) =>
  base({ event_id: ev, message_id: mid, turn_id: turn, kind: 'tool_call', tool_name: 'Workflow', tool_use_id: tu,
    payload: { tool_name: 'Workflow', input: { script: `export const meta = { name: '${name}' }` } } });
// turn 단 사이드체인(사이드카 없음): agent_id + turn_id만
const wfUser = (ag: string, ev: string, turn: string) =>
  base({ event_id: ev, kind: 'user_message', is_sidechain: true, agent_id: ag, turn_id: turn, payload: { content: 'prompt' } });
const wfAsst = (ag: string, ev: string, turn: string, text: string) =>
  base({ event_id: ev, kind: 'assistant_message', is_sidechain: true, agent_id: ag, turn_id: turn, payload: { text } });
```

- [ ] **Step 2: 실패 테스트**

```ts
it('Workflow tool_call + 같은 turn 사이드체인 에이전트 → WorkflowGroup', () => {
  const evs = [
    asstMain('m1', '워크플로우 실행'),
    wfCall('m1', 'wfc', 'tu-wf', 't1', 'review-changes'),
    wfUser('A', 'au', 't1'), wfUser('B', 'bu', 't1'),
    wfAsst('A', 'a1', 't1', 'A 결론'), wfAsst('B', 'b1', 't1', 'B 결론'),
    asstMain('m2', '워크플로우 종합 X'),
  ];
  const items = buildStreamModel(evs);
  const wf = items.find((i) => i.type === 'workflow-group');
  expect(wf).toBeTruthy();
  expect(wf.name).toBe('review-changes');
  expect(wf.taskEventId).toBe('wfc');
  expect(wf.agentGroups.map((g) => g.agentId).sort()).toEqual(['A', 'B']);
  expect(wf.synthesis).toContain('종합');
  expect(wf.settled).toBe(true);
});
it('사이드카 있는 Agent-배치는 여전히 BatchGroup (Workflow로 흡수 안 됨)', () => {
  const evs = [
    asstMain('m1','병렬'), taskCall('m1','tcA','tuA'), taskCall('m1','tcB','tuB'),
    sidecar('A','tuA','Explore','A'), sidecar('B','tuB','Explore','B'),
    scAsst('A','a1','A끝'), scAsst('B','b1','B끝'),
  ];
  const items = buildStreamModel(evs);
  expect(items.some((i) => i.type === 'batch-group')).toBe(true);
  expect(items.some((i) => i.type === 'workflow-group')).toBe(false);
});
```

- [ ] **Step 3: 실패 확인** — Run: `npx vitest run ...buildStreamModel... -t WorkflowGroup` · Expected: FAIL.

- [ ] **Step 4: prepass — Workflow call 수집 + per-agent turn/start** — `buildStreamModel` 초반 prepass 루프(`for (const e of events)` 사이드카 수집부) 에 추가:

```ts
// turn_id -> 그 turn의 Workflow tool_call들(시간순). 에이전트는 자신의 turn_id에서
// 시작 시각 이전의 가장 늦은 Workflow call에 귀속(한 turn 다중 Workflow 대비).
const wfCallsByTurn = new Map<string, { eventId: string; at: string; name: string | null; description: string | null }[]>();
for (const e of events) {
  if (e.kind === 'tool_call' && e.tool_name === 'Workflow' && e.turn_id) {
    const input = asObj(asObj(e.payload).input);
    const meta = parseWorkflowMeta(typeof input.script === 'string' ? input.script : '');
    const arr = wfCallsByTurn.get(e.turn_id) ?? [];
    arr.push({ eventId: e.event_id, at: e.observed_at, ...meta });
    wfCallsByTurn.set(e.turn_id, arr);
  }
}
for (const arr of wfCallsByTurn.values()) arr.sort((a, b) => a.at.localeCompare(b.at));
```

per-agent turn/start 캡처: collection 맵 옆에 추가 후 `emitSidechain`이 받도록 main 루프에서 채운다. `scBufs` 선언부 근처:
```ts
const scTurnByKey = new Map<string, string | null>();
const scStartByKey = new Map<string, string>();
```
main 루프의 sidechain 분기(아래 `emit(...)` 호출 직전, `c.cat==='message'|'thinking'|'activity'` 공통 경로)에서 raw `e` 기준으로 채운다 — 가장 간단히 `emit` 호출 전 한 줄:
```ts
if (sc && agent) {
  if (!scTurnByKey.has(agent)) scTurnByKey.set(agent, e.turn_id ?? null);
  const prev = scStartByKey.get(agent);
  if (!prev || e.observed_at < prev) scStartByKey.set(agent, e.observed_at);
}
```
(message/thinking/activity 세 분기 공통 진입점인 `for (const e of events)` 본문 상단, `const c = classify(e)` 바로 다음에 두면 한 번만 작성된다.)

- [ ] **Step 5: flush 라우팅 — message_id → workflow → solo** — `flushSidechain`의 "2) message_id로 그룹핑" 블록을 교체. 각 SidechainGroup을 (a) 사이드카 message_id, (b) turn의 Workflow call, (c) solo 순으로 버킷팅:

```ts
const byKey = new Map<string, { kind: 'batch' | 'wf' | 'solo'; wf?: { eventId: string; name: string | null; description: string | null }; sibs: SidechainGroup[] }>();
const keyOrder: string[] = [];
const put = (key: string, kind: 'batch' | 'wf' | 'solo', g: SidechainGroup, wf?: any) => {
  let b = byKey.get(key);
  if (!b) { b = { kind, sibs: [], wf }; byKey.set(key, b); keyOrder.push(key); }
  b.sibs.push(g);
};
for (const g of groups) {
  const tu = g.agentId ? metaByAgent.get(g.agentId)?.toolUseId ?? null : null;
  const mid = tu ? callMsgByUse.get(tu) ?? null : null;
  if (mid) { put(`msg-${mid}`, 'batch', g); continue; }
  const turn = g.agentId ? scTurnByKey.get(g.agentId) ?? null : null;
  const start = g.agentId ? scStartByKey.get(g.agentId) ?? '' : '';
  const calls = turn ? wfCallsByTurn.get(turn) ?? [] : [];
  // 시작 시각 이전의 가장 늦은 Workflow call (없으면 turn의 첫 call)
  let chosen = null as null | { eventId: string; at: string; name: string | null; description: string | null };
  for (const c of calls) { if (c.at <= start) chosen = c; }
  if (!chosen && calls.length) chosen = calls[0];
  if (chosen) { put(`wf-${chosen.eventId}`, 'wf', g, chosen); continue; }
  put(`solo-${g.id}`, 'solo', g);
}
```

- [ ] **Step 6: materialize — batch / workflow / solo** — 같은 함수의 "3) N>=2 → BatchGroup" 루프를 교체:

```ts
for (const key of keyOrder) {
  const b = byKey.get(key)!;
  if (b.kind === 'wf') {
    const wg: WorkflowGroup = {
      type: 'workflow-group',
      id: `wf-${b.sibs[0].id}`,
      name: b.wf?.name ?? null,
      description: b.wf?.description ?? null,
      taskEventId: b.wf?.eventId ?? null,
      agentGroups: b.sibs,
      synthesis: null,
      settled: b.sibs.every((s) => s.conclusion != null),
    };
    items.push(wg);
    pendingSynthesis = wg;
  } else if (b.kind === 'batch' && b.sibs.length >= 2) {
    const batch: BatchGroup = { type: 'batch-group', id: `batch-${b.sibs[0].id}`, agentGroups: b.sibs, synthesis: null, settled: b.sibs.every((s) => s.conclusion != null) };
    items.push(batch);
    pendingSynthesis = batch;
  } else {
    items.push(b.sibs[0]);
  }
}
```

- [ ] **Step 7: synthesis 일반화** — `let pendingBatch: BatchGroup | null = null;` 을 `let pendingSynthesis: BatchGroup | WorkflowGroup | null = null;` 로 바꾸고, `fillPendingSynthesis`가 `pendingSynthesis`를 채우고 null로 비우도록 이름/참조 교체(동작 동일). main assistant_message 처리부의 `fillPendingSynthesis(c.text!)` 호출은 그대로.

- [ ] **Step 8: 통과 확인** — Run: `npx vitest run src/components/replay/stream/__tests__/buildStreamModel.test.ts` · Expected: PASS (신규 2개 + 기존 전체 회귀 없음).

- [ ] **Step 9: 커밋**
```bash
git add -A && git commit -m "feat(webui): Workflow fan-out을 turn_id로 WorkflowGroup 라우팅"
```

---

## Task 3: workflowStats / timeline / heat 헬퍼

**Files:** Create `workflowStats.ts`, `__tests__/workflowStats.test.ts`

- [ ] **Step 1: 실패 테스트** — `__tests__/workflowStats.test.ts`:

```ts
import { describe, it, expect } from 'vitest';
import { groupSpanMs, workflowStats, workflowTimeline, agentDurationHeat } from '../workflowStats';
import type { SidechainGroup } from '../streamModel';

const g = (id: string, startIso: string, endIso: string, concl: string | null): SidechainGroup => ({
  type: 'sidechain-group', id, agentId: id, agentType: 'Explore', description: null, taskEventId: null, conclusion: concl,
  items: [
    { type: 'message', id: id + 'u', eventId: id + 'u', role: 'user', model: null, text: 'p', timestamp: startIso, sidechain: true },
    { type: 'message', id: id + 'a', eventId: id + 'a', role: 'assistant', model: null, text: concl ?? '', timestamp: endIso, sidechain: true },
  ],
});

describe('workflow helpers', () => {
  const groups = [
    g('A', '2026-06-14T00:00:00Z', '2026-06-14T00:38:00Z', 'a'), // 38m
    g('B', '2026-06-14T00:00:00Z', '2026-06-14T00:24:00Z', 'b'), // 24m, overlaps A
    g('C', '2026-06-14T00:00:00Z', '2026-06-14T00:02:00Z', 'c'), // 2m
  ];
  it('groupSpanMs = 자식 min~max', () => { expect(groupSpanMs(groups[0])).toBe(38 * 60000); });
  it('maxConcurrency / longest / median', () => {
    const s = workflowStats(groups);
    expect(s.agentCount).toBe(3);
    expect(s.maxConcurrency).toBe(3);   // 0~2m 구간 셋 다 실행
    expect(s.longestMs).toBe(38 * 60000);
    expect(s.medianMs).toBe(24 * 60000);
    expect(s.incomplete).toBe(0);
  });
  it('timeline lanes: 상대 시작·소요', () => {
    const t = workflowTimeline(groups);
    expect(t.spanMs).toBe(38 * 60000);
    expect(t.lanes[0]).toMatchObject({ startMs: 0, durMs: 38 * 60000 });
  });
  it('agentDurationHeat: ≥5m warn, ≥20m hot', () => {
    expect(agentDurationHeat(60000)).toBe('');
    expect(agentDurationHeat(6 * 60000)).toBe('warn');
    expect(agentDurationHeat(25 * 60000)).toBe('hot');
  });
});
```

- [ ] **Step 2: 실패 확인** — Run: `npx vitest run src/components/replay/stream/__tests__/workflowStats.test.ts` · Expected: FAIL (모듈 없음).

- [ ] **Step 3: 구현** — `workflowStats.ts`:

```ts
import type { SidechainGroup } from './streamModel';

/** 한 에이전트 그룹의 [최초, 최후] 관측 타임스탬프 폭(ms). */
export function groupSpanMs(group: SidechainGroup): number {
  let min = Infinity, max = -Infinity;
  const see = (iso: string) => { const t = new Date(iso).getTime(); if (!Number.isNaN(t)) { min = Math.min(min, t); max = Math.max(max, t); } };
  for (const it of group.items) {
    if (it.type === 'message') see(it.timestamp);
    else if (it.type === 'activity-run') for (const ae of it.events) see(ae.event.observed_at);
    else if (it.type === 'thinking') for (const e of it.events) see(e.timestamp);
  }
  return max > min ? max - min : 0;
}

function bounds(group: SidechainGroup): { start: number; end: number } {
  let min = Infinity, max = -Infinity;
  const see = (iso: string) => { const t = new Date(iso).getTime(); if (!Number.isNaN(t)) { min = Math.min(min, t); max = Math.max(max, t); } };
  for (const it of group.items) {
    if (it.type === 'message') see(it.timestamp);
    else if (it.type === 'activity-run') for (const ae of it.events) see(ae.event.observed_at);
    else if (it.type === 'thinking') for (const e of it.events) see(e.timestamp);
  }
  return { start: min === Infinity ? 0 : min, end: max === -Infinity ? 0 : max };
}

export interface WorkflowStat { agentCount: number; maxConcurrency: number; longestMs: number; medianMs: number; incomplete: number; }

export function workflowStats(groups: SidechainGroup[]): WorkflowStat {
  const durs = groups.map(groupSpanMs).sort((a, b) => a - b);
  const bnds = groups.map(bounds);
  // 동시성: 시작 +1 / 종료 -1 스윕
  const pts: [number, number][] = [];
  for (const b of bnds) { pts.push([b.start, 1]); pts.push([b.end, -1]); }
  pts.sort((a, b) => a[0] - b[0] || a[1] - b[1]);
  let cur = 0, max = 0;
  for (const [, d] of pts) { cur += d; if (cur > max) max = cur; }
  const median = durs.length ? durs[Math.floor((durs.length - 1) / 2)] : 0;
  return {
    agentCount: groups.length,
    maxConcurrency: max,
    longestMs: durs.length ? durs[durs.length - 1] : 0,
    medianMs: median,
    incomplete: groups.filter((g) => g.conclusion == null).length,
  };
}

export interface GanttLane { id: string; label: string; startMs: number; durMs: number; }
export interface WorkflowTimeline { spanMs: number; lanes: GanttLane[]; }

/** 첫 시작을 0으로 한 상대 타임라인. 라벨 = description ?? prompt 첫 줄 ?? agentType ?? id. */
export function workflowTimeline(groups: SidechainGroup[]): WorkflowTimeline {
  const bnds = groups.map((g) => ({ g, ...bounds(g) }));
  const origin = Math.min(...bnds.map((b) => b.start).filter((n) => n > 0), Infinity);
  const base = Number.isFinite(origin) ? origin : 0;
  let spanMs = 0;
  const lanes = bnds.map(({ g, start, end }) => {
    const startMs = Math.max(0, start - base);
    const durMs = Math.max(0, end - start);
    spanMs = Math.max(spanMs, startMs + durMs);
    const promptLine = g.items.find((i) => i.type === 'message' && i.role === 'user');
    const label = g.description
      ?? (promptLine && promptLine.type === 'message' ? promptLine.text.split('\n', 1)[0].trim().slice(0, 22) : '')
      || g.agentType || g.agentId || g.id;
    return { id: g.id, label, startMs, durMs };
  });
  return { spanMs, lanes };
}

/** Workflow 에이전트(분 단위) 소요 heat — tool-exec용 durationHeat(10s/60s)와 별개. */
export function agentDurationHeat(ms: number): '' | 'warn' | 'hot' {
  if (ms >= 20 * 60000) return 'hot';
  if (ms >= 5 * 60000) return 'warn';
  return '';
}
```

- [ ] **Step 4: 통과 확인** — Run 동일 · Expected: PASS.

- [ ] **Step 5: 커밋**
```bash
git add -A && git commit -m "feat(webui): workflow 통계·타임라인·heat 헬퍼"
```

---

## Task 4: WorkflowGroup 컴포넌트 (접힘=종합+통계+간트, 펼침=+자식)

**Files:** Create `WorkflowGroup.tsx`, `WorkflowGroup.module.css`, `__tests__/WorkflowGroup.test.tsx`

- [ ] **Step 1: 실패 테스트** — `__tests__/WorkflowGroup.test.tsx`:

```tsx
import { describe, it, expect } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { WorkflowGroup } from '../WorkflowGroup';
import type { WorkflowGroup as WG, SidechainGroup } from '../streamModel';

const child = (id: string, end: string): SidechainGroup => ({
  type: 'sidechain-group', id, agentId: id, agentType: 'Explore', description: id + ' 작업', taskEventId: null, conclusion: id + ' 결론',
  items: [
    { type: 'message', id: id + 'u', eventId: id + 'u', role: 'user', model: null, text: 'prompt', timestamp: '2026-06-14T00:00:00Z', sidechain: true },
    { type: 'message', id: id + 'a', eventId: id + 'a', role: 'assistant', model: null, text: id + ' 결론', timestamp: end, sidechain: true },
  ],
});
const wg: WG = { type: 'workflow-group', id: 'wf1', name: 'review-changes', description: null, taskEventId: 'wfc',
  agentGroups: [child('A', '2026-06-14T00:38:00Z'), child('B', '2026-06-14T00:02:00Z')], synthesis: '종합 결과 X', settled: true };

const noop = () => {};
describe('WorkflowGroup', () => {
  it('접힘 기본: 워크플로우명·종합·통계칩·미니간트 모두 노출, 자식 행 숨김', () => {
    render(<WorkflowGroup group={wg} selectedEventId={null} onSelect={noop} findingEventIds={new Set()} />);
    expect(screen.getByTestId('workflow-group')).toHaveAttribute('data-expanded', 'false');
    expect(screen.getByText('review-changes')).toBeInTheDocument();
    expect(screen.getByTestId('wf-synthesis')).toHaveTextContent('종합');
    expect(screen.getByTestId('wf-stats')).toHaveTextContent('최대 병렬');
    expect(screen.getAllByTestId('wf-lane').length).toBe(2);   // 미니간트 레인 = 에이전트 수
    expect(screen.queryByTestId('subagent-group')).toBeNull(); // 자식은 펼쳐야
  });
  it('펼치면 자식 SubagentGroup 노출', () => {
    render(<WorkflowGroup group={wg} selectedEventId={null} onSelect={noop} findingEventIds={new Set()} />);
    fireEvent.click(screen.getByTestId('wf-toggle'));
    expect(screen.getAllByTestId('subagent-group').length).toBe(2);
  });
});
```

- [ ] **Step 2: 실패 확인** — Run: `npx vitest run src/components/replay/stream/__tests__/WorkflowGroup.test.tsx` · Expected: FAIL (모듈 없음).

- [ ] **Step 3: 컴포넌트 구현** — `WorkflowGroup.tsx` (BatchGroup 패턴 + 항상보이는 통계/간트):

```tsx
import { useMemo, useState } from 'react';
import { ChevronDown, ChevronRight, Workflow as WorkflowIcon } from 'lucide-react';
import { SubagentGroup } from './SubagentGroup';
import { formatDuration } from './duration';
import { workflowStats, workflowTimeline, agentDurationHeat } from './workflowStats';
import type { WorkflowGroup as WorkflowGroupModel, SidechainGroup, StreamItem } from './streamModel';
import styles from './WorkflowGroup.module.css';

interface Props { group: WorkflowGroupModel; selectedEventId: string | null; onSelect: (id: string) => void; findingEventIds: Set<string>; }

function itemEventIds(it: StreamItem): string[] {
  if (it.type === 'message') return [it.eventId];
  if (it.type === 'thinking') return it.events.map((e) => e.eventId);
  if (it.type === 'activity-run') return it.events.map((ae) => ae.event.event_id);
  if (it.type === 'batch-group' || it.type === 'workflow-group') return it.agentGroups.flatMap(itemEventIds);
  return it.items.flatMap(itemEventIds);
}

export function WorkflowGroup({ group, selectedEventId, onSelect, findingEventIds }: Props) {
  const [userOverride, setUserOverride] = useState<boolean | null>(null);
  const containsSelected = selectedEventId != null && group.agentGroups.some((g) => itemEventIds(g).includes(selectedEventId));
  const expanded = userOverride ?? (containsSelected || !group.settled);
  const stats = useMemo(() => workflowStats(group.agentGroups), [group.agentGroups]);
  const tl = useMemo(() => workflowTimeline(group.agentGroups), [group.agentGroups]);
  const pct = (n: number) => (tl.spanMs > 0 ? (n / tl.spanMs) * 100 : 0);

  return (
    <section data-testid="workflow-group" data-expanded={String(expanded)} className={styles.group}>
      <div className={styles.headerRow}>
        <button data-testid="wf-toggle" className={styles.header} onClick={() => setUserOverride(!expanded)} aria-expanded={expanded}>
          {expanded ? <ChevronDown size={13} className={styles.chevron} /> : <ChevronRight size={13} className={styles.chevron} />}
          <WorkflowIcon size={13} className={styles.icon} aria-hidden />
          <span className={styles.chip}>워크플로우</span>
          <span className={styles.name}>{group.name ?? '워크플로우'}</span>
          <span className={styles.meta}>
            <span>{stats.agentCount} agents</span>
            <span className={styles.status}>{group.settled ? `✓ ${stats.agentCount}/${stats.agentCount}` : '⏳'}</span>
            {tl.spanMs > 0 && <span className={styles.duration} data-heat={agentDurationHeat(stats.longestMs)}>{formatDuration(tl.spanMs)}</span>}
          </span>
        </button>
      </div>

      <div data-testid="wf-synthesis" className={styles.synthesis}>
        <span className={styles.synthesisLabel}>종합</span>
        <span>{group.synthesis || '진행 중'}</span>
      </div>

      <div data-testid="wf-stats" className={styles.stats}>
        <span className={styles.stat}>최대 병렬 <b>{stats.maxConcurrency}</b></span>
        <span className={styles.stat} data-heat={agentDurationHeat(stats.longestMs)}>최장 <b>{formatDuration(stats.longestMs)}</b></span>
        <span className={styles.stat}>중앙값 <b>{formatDuration(stats.medianMs)}</b></span>
        {stats.incomplete > 0 && <span className={styles.stat}>미완 <b>{stats.incomplete}</b></span>}
      </div>

      {/* 항상 보이는 컴팩트 미니 간트 */}
      <div className={styles.gantt}>
        {tl.lanes.map((l) => (
          <div key={l.id} data-testid="wf-lane" className={styles.lane}>
            <span className={styles.laneLabel} title={l.label}>{l.label}</span>
            <div className={styles.track}>
              <div className={styles.bar} data-heat={agentDurationHeat(l.durMs)}
                   style={{ left: `${pct(l.startMs)}%`, width: `${Math.max(1.5, pct(l.durMs))}%` }}>
                <span className={styles.barLabel}>{formatDuration(l.durMs)}</span>
              </div>
            </div>
          </div>
        ))}
      </div>

      {expanded && (
        <div className={styles.body}>
          {group.agentGroups.map((g) => (
            <SubagentGroup key={g.id} group={g} selectedEventId={selectedEventId} onSelect={onSelect} findingEventIds={findingEventIds} />
          ))}
        </div>
      )}
    </section>
  );
}
```

- [ ] **Step 4: CSS** — `WorkflowGroup.module.css` (BatchGroup.module.css 복제 후 색을 `--wimcc-lane-quality`(orange)로, 통계/간트 규칙 추가):

```css
.group { margin: 8px 0 8px 24px; padding-left: 10px; border-left: 2px solid var(--wimcc-lane-quality, #ff8a4c); }
.headerRow { display: flex; align-items: center; gap: 4px; }
.header { display: flex; align-items: center; gap: 6px; flex: 1 1 auto; min-width: 0; overflow: hidden; padding: 4px 6px 4px 0; background: none; border: none; cursor: pointer; text-align: left; font-family: inherit; font-size: 11px; font-weight: 600; color: var(--wimcc-lane-quality, #ff8a4c); border-radius: 4px; }
.header:hover { background: var(--wimcc-surface-2, #161a23); }
.chevron { flex: none; color: var(--wimcc-fg-subtle, #6a7180); }
.icon { flex: none; }
.chip { flex: none; padding: 2px 7px; border-radius: 4px; font-weight: 600; font-size: 10px; line-height: 1.4; color: var(--wimcc-lane-quality, #ff8a4c); background: var(--wimcc-surface-3, #1b202a); border: 1px solid var(--wimcc-lane-quality, #ff8a4c); }
.name { flex: none; color: var(--wimcc-fg, #e6e8ee); font-weight: 600; }
.meta { flex: none; margin-left: auto; display: flex; align-items: center; gap: 8px; font-weight: 400; color: var(--wimcc-fg-subtle, #6a7180); }
.status { font-variant-numeric: tabular-nums; font-weight: 600; color: var(--wimcc-fg-muted, #aab0bd); }
.duration { font-variant-numeric: tabular-nums; }
.duration[data-heat='warn'] { color: var(--wimcc-warning, #f0b429); }
.duration[data-heat='hot'] { color: var(--wimcc-danger, #ef4747); font-weight: 600; }
.synthesis { display: flex; align-items: baseline; gap: 6px; margin: 2px 0 4px 19px; font-size: 11px; color: var(--wimcc-fg-muted, #aab0bd); }
.synthesisLabel { flex: none; font-weight: 600; font-size: 9px; letter-spacing: 0.04em; text-transform: uppercase; color: var(--wimcc-lane-quality, #ff8a4c); }
.stats { display: flex; flex-wrap: wrap; gap: 6px; margin: 0 0 6px 19px; }
.stat { font-size: 10px; color: var(--wimcc-fg-muted, #aab0bd); background: var(--wimcc-surface-2, #161a23); border: 1px solid var(--wimcc-border, #1d212c); border-radius: 4px; padding: 2px 7px; }
.stat b { color: var(--wimcc-fg, #e6e8ee); font-variant-numeric: tabular-nums; }
.stat[data-heat='hot'] b { color: var(--wimcc-danger, #ef4747); }
.gantt { margin: 2px 0 4px 19px; }
.lane { display: flex; align-items: center; height: 15px; margin: 2px 0; }
.laneLabel { flex: none; width: 120px; padding-right: 8px; text-align: right; color: var(--wimcc-fg-muted, #aab0bd); font-size: 10px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.track { position: relative; flex: 1 1 auto; height: 100%; }
.bar { position: absolute; top: 50%; transform: translateY(-50%); height: 7px; border-radius: 3px; background: var(--wimcc-lane-context, #b07dff); box-shadow: inset 0 0 0 1px rgba(0,0,0,.25); }
.bar[data-heat='warn'] { background: var(--wimcc-warning, #f0b429); }
.bar[data-heat='hot'] { background: var(--wimcc-danger, #ef4747); }
.barLabel { position: absolute; left: calc(100% + 5px); top: 50%; transform: translateY(-50%); font-size: 9px; color: var(--wimcc-fg-subtle, #6a7180); white-space: nowrap; font-variant-numeric: tabular-nums; }
.body { display: flex; flex-direction: column; }
```

- [ ] **Step 5: 통과 확인** — Run 동일 · Expected: PASS.

- [ ] **Step 6: 커밋**
```bash
git add -A && git commit -m "feat(webui): WorkflowGroup 컴포넌트(접힘 종합+통계+미니간트)"
```

---

## Task 5: ConversationStream 통합

**Files:** Modify `ConversationStream.tsx`, `SubagentGroup.tsx`, `BatchGroup.tsx`, `__tests__/ConversationStream.test.tsx`

- [ ] **Step 1: 실패 테스트** — `ConversationStream.test.tsx`에 추가(기존 batch fixture 패턴 따름):

```tsx
it('workflow-group 렌더 + 자식 이벤트 contains 판정', () => {
  const wf = { type: 'workflow-group', id: 'wf1', name: 'review', description: null, taskEventId: 'wfc', synthesis: null, settled: false,
    agentGroups: [{ type: 'sidechain-group', id: 'A', agentId: 'A', agentType: 'Explore', description: null, taskEventId: null, conclusion: null,
      items: [{ type: 'message', id: 'a1', eventId: 'a1', role: 'assistant', model: null, text: 'x', timestamp: '2026-06-14T00:00:00Z', sidechain: true }] }] };
  render(<ConversationStream items={[wf as any]} selectedEventId={'a1'} onSelect={() => {}} findingEventIds={new Set()} />);
  expect(screen.getByTestId('workflow-group')).toBeInTheDocument();
});
```

- [ ] **Step 2: 실패 확인** — Run: `npx vitest run src/components/replay/stream/__tests__/ConversationStream.test.tsx -t workflow-group` · Expected: FAIL.

- [ ] **Step 3: 구현** — `ConversationStream.tsx`:
  - import: `import { WorkflowGroup } from './WorkflowGroup';`
  - `itemContainsEvent`에 추가(line 52 batch 분기 옆):
```ts
  if (item.type === 'workflow-group') return item.agentGroups.some((g) => itemContainsEvent(g, eventId));
```
  - `renderItem`에 추가(batch-group 분기 옆, line 375 인근):
```tsx
    if (item.type === 'workflow-group') {
      return <WorkflowGroup group={item} selectedEventId={selectedEventId} onSelect={onSelect} findingEventIds={findingEventIds} />;
    }
```
  - `SubagentGroup.tsx`·`BatchGroup.tsx`의 `itemEventIds`에서 batch 분기를 `if (it.type === 'batch-group' || it.type === 'workflow-group') return it.agentGroups.flatMap(itemEventIds);` 로 확장.

- [ ] **Step 4: 통과 확인** — Run: `cd webui && npx vitest run` (전체) · Expected: PASS, 회귀 없음.

- [ ] **Step 5: 커밋**
```bash
git add -A && git commit -m "feat(webui): ConversationStream에 WorkflowGroup 렌더·contains"
```

---

## Task 6: 브라우저 smoke (CLAUDE.md 의무)

**Files:** 없음(검증 전용)

- [ ] **Step 1:** `cd webui && npm run build` 또는 vite dev(:5173). 백엔드는 **현재 HEAD로 재빌드된** `target/release/wimcc serve`(사이드카 기능 포함) 사용.
- [ ] **Step 2:** Workflow가 있는 세션을 브라우저로 열어 확인:
  - 정적 검증: `653ea169`(turn 9112ef42, witmcc-ux-investigate 5 agents) / `89f99c17`(be6dca26, insight-redesign-evidence 9 agents) 딥링크. 단, 활성 세션 SSE 간섭 회피 위해 자동스크롤 off 후 확인(메모리 `witmcc-smoke-use-static-session`).
  - 라이브 검증: 현재 세션에서 `Workflow`로 간단한 fan-out 실행 → 재ingest → 접힘에 종합+통계+미니간트, 펼침에 자식. claude-in-chrome 스크린샷.
- [ ] **Step 3:** 확인 항목 — 접힘에 종합·통계칩·미니간트 노출 / straggler 빨강 / 펼침 자식 SubagentGroup / Agent-배치는 여전히 teal BatchGroup(회귀 없음) / Workflow 점프(taskEventId) 동작.

---

## Self-Review

- **Spec coverage:** 승인 프로토타입 A안(접힘=헤더+종합+통계+미니간트, 펼침=+자식) = Task4 · turn_id 그룹핑 = Task2 · 통계/간트 = Task3 · 이름 파싱 = Task1 · 통합 = Task5 · 브라우저 = Task6. Agent-배치 비흡수 회귀가드 = Task2 Step2 둘째 테스트.
- **Placeholder scan:** 코드 스텝 전부 실제 코드. 라벨 도출(workflowTimeline)·heat 임계는 명시.
- **Type consistency:** `WorkflowGroup{type,id,name,description,taskEventId,agentGroups,synthesis,settled}` — Task1 정의가 Task2(생성)·Task4(소비)·Task5(렌더)에서 동일. `pendingSynthesis`(구 pendingBatch) 이름 일관. `workflowStats`/`workflowTimeline`/`agentDurationHeat` 시그니처 Task3↔Task4 일치.
- **주의(실측 한계):** ① 미니간트 레인 라벨은 사이드카 description 부재 시 prompt 첫 줄인데, Workflow 프롬프트는 템플릿 prefix("You are investigating…")를 공유해 비차별적일 수 있음 → 펼침 자식 행이 진짜 정체성. v2에서 라벨 도출 개선 후속. ② stage/배리어 경계는 본 plan 미구현(타임라인 막대 오프셋으로 시각적으로만 드러남).
