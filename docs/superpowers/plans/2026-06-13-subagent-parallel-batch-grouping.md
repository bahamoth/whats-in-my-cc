# 서브에이전트 병렬 배치 그룹핑 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** replay 스트림에서 같은 턴에 병렬 디스패치된 서브에이전트들을 "병렬 배치" 컨테이너로 묶고, agent_id로 전역 수집(de-interleave)하며, 2단계 접기(L0 배치+종합 / L1 에이전트 요약 / L2 상세)로 보여준다.

**Architecture:** `buildStreamModel`(webui)이 sidechain 이벤트를 agent 경계마다 끊던 것을 **agent_id별 버퍼로 전역 수집** → 형제 `SidechainGroup`들을 **디스패치 메시지(message_id)** 기준으로 `BatchGroup`으로 래핑. 결론=각 agent의 마지막 assistant_message, 종합=배치 후 main assistant_message. 새 `BatchGroup` 컴포넌트가 L0/L1을, 기존 `SubagentGroup`이 L2를 담당. 순수 webui 변경(백엔드 DTO에 키 이미 존재).

**Tech Stack:** TypeScript, React, @tanstack/react-virtual, vitest. 설계 근거: `docs/superpowers/specs/2026-06-13-subagent-parallel-batch-grouping-design.md`.

**기준 데이터(real-data anchored, 표본 1 세션):** `fb6b8e3a-2289-4214-884c-0c721a3e3cf5` — 한 메시지 `msg_01WPxhd…`에서 `Agent` 5개 디스패치 → 형제 서브에이전트 5개 병렬, 시간 교차. 전 DB: 디스패치 132건 전부 main, 중첩 0.

---

## File Structure

- `webui/src/components/replay/stream/streamModel.ts` (modify) — 타입(`BatchGroup`, `SidechainGroup.conclusion`), 그룹핑 알고리즘(전역 수집 + 배치 래핑 + 결론/종합).
- `webui/src/components/replay/stream/BatchGroup.tsx` (create) — 배치 컨테이너(L0 접힘=배치+종합, L1 펼침=자식 `SubagentGroup` 목록 + outcome).
- `webui/src/components/replay/stream/BatchGroup.module.css` (create) — 스타일(토큰 사용).
- `webui/src/components/replay/stream/SubagentGroup.tsx` (modify) — 축약 줄에 결론, 진행/완료 상태, 기본 접힘(L2 펼침은 기존).
- `webui/src/components/replay/stream/ConversationStream.tsx` (modify) — `renderItem`에 `batch-group`, `itemContainsEvent` 재귀.
- `webui/src/components/replay/stream/__tests__/buildStreamModel.test.ts` (modify) — 배치/결론/종합/degrade 테스트.
- `webui/src/components/replay/stream/__tests__/BatchGroup.test.tsx` (create) — 컴포넌트 렌더 테스트.

---

## Task 1: 타입 — BatchGroup · SidechainGroup.conclusion

**Files:** Modify `webui/src/components/replay/stream/streamModel.ts`

- [ ] **Step 1: 실패 테스트 (타입 컴파일 + 빌더 산출)**

`__tests__/buildStreamModel.test.ts`에 추가:
```ts
it('병렬 형제는 BatchGroup으로 래핑되고 자식은 agent별 SidechainGroup', () => {
  const evs = [
    asstMain('m1', '병렬로 2개'),                  // main assistant
    taskCall('m1', 'tc-A', 'tu-A'), taskCall('m1', 'tc-B', 'tu-B'),
    sidecar('A', 'tu-A', 'Explore', '조사 A'), sidecar('B', 'tu-B', 'general', '조사 B'),
    // 교차 도착
    scUser('A', 'pA'), scUser('B', 'pB'),
    scAsst('A', 'a1', 'A 중간'), scAsst('B', 'b1', 'B 중간'),
    scAsst('A', 'a2', 'A 결론'), scAsst('B', 'b2', 'B 결론'),
  ];
  const items = buildStreamModel(evs);
  const batch = items.find((i) => i.type === 'batch-group');
  expect(batch).toBeTruthy();
  expect(batch.agentGroups).toHaveLength(2);
  expect(batch.agentGroups.map((g) => g.agentId).sort()).toEqual(['A', 'B']);
});
```
(테스트 헬퍼 `asstMain/taskCall/sidecar/scUser/scAsst`는 Task 2 Step 1에서 정의 — 먼저 읽지 말고 함께 추가.)

- [ ] **Step 2: 실패 확인** — Run: `cd webui && npx vitest run src/components/replay/stream/__tests__/buildStreamModel.test.ts -t 'BatchGroup으로 래핑'` · Expected: FAIL (`batch-group` 타입 없음/`batch` undefined).

- [ ] **Step 3: 타입 추가**

`streamModel.ts`의 `SidechainGroup`에 필드 추가:
```ts
  /** 그 agent의 마지막 assistant_message 요약 — 축약 줄의 "결론". null=미관측/진행중. */
  conclusion: string | null;
```
`StreamItem` union 위에 새 인터페이스:
```ts
/** 한 디스패치 턴(같은 message_id)에서 병렬로 띄운 형제 서브에이전트 묶음. 동시성은
 *  에이전트 사이에만 있고 각 자식은 직렬이므로, agent_id로 전역 수집한 SidechainGroup을
 *  배치로 래핑한다(시간축은 배치=한 슬롯으로 보존). 단일 디스패치(N=1)는 래핑하지 않는다. */
export interface BatchGroup {
  type: 'batch-group';
  id: string;
  agentGroups: SidechainGroup[];
  /** 배치 후 main의 첫 assistant_message 요약 = 종합 결과. null=진행 중/미관측. */
  synthesis: string | null;
  /** 전부 완료 추정(모든 자식이 결론 보유)이면 true. 스트리밍 중 false. */
  settled: boolean;
}
```
union 갱신:
```ts
export type StreamItem = MessageItem | ActivityRun | SidechainGroup | ThinkingMarker | BatchGroup;
```

- [ ] **Step 4: 통과 확인** — 타입은 컴파일되나 빌더가 아직 batch를 안 만들어 테스트는 여전히 FAIL(다음 Task에서 green). Run: `npx tsc --noEmit`(webui) · Expected: 타입 에러 없음.

- [ ] **Step 5: 커밋**
```bash
git add webui/src/components/replay/stream/streamModel.ts webui/src/components/replay/stream/__tests__/buildStreamModel.test.ts
git commit -m "test(webui): 병렬 배치 그룹핑 red — BatchGroup 타입"
```

---

## Task 2: 테스트 헬퍼 + 결론 추출(agent별 마지막 assistant_message)

**Files:** Modify `__tests__/buildStreamModel.test.ts`, `streamModel.ts`

- [ ] **Step 1: 테스트 헬퍼 정의** (파일 상단, 기존 헬퍼 옆)

```ts
const base = (over: Partial<ObservedEventDto>): ObservedEventDto => ({
  event_id: 'e', session_id: 's', kind: 'assistant_message', actor: 'assistant',
  observed_at: '2026-06-13T00:00:00.000Z', is_sidechain: false, agent_id: '',
  message_id: null, turn_id: null, tool_use_id: null, subkind: null, payload: {},
  ...over,
} as ObservedEventDto);
const asstMain = (mid: string, text: string) => base({ event_id: mid, message_id: mid, kind: 'assistant_message', payload: { text } });
const taskCall = (mid: string, ev: string, tu: string) => base({ event_id: ev, message_id: mid, kind: 'tool_call', tool_name: 'Agent', tool_use_id: tu });
const sidecar = (ag: string, tu: string, atype: string, desc: string) => base({ event_id: `meta-${ag}`, kind: 'attachment_meta', subkind: 'subagent_meta', is_sidechain: true, agent_id: ag, tool_use_id: tu, payload: { agentType: atype, description: desc, toolUseId: tu } });
const scUser = (ag: string, ev: string) => base({ event_id: ev, kind: 'user_message', is_sidechain: true, agent_id: ag, payload: { content: 'prompt' } });
const scAsst = (ag: string, ev: string, text: string) => base({ event_id: ev, kind: 'assistant_message', is_sidechain: true, agent_id: ag, payload: { text } });
```

- [ ] **Step 2: 결론 추출 실패 테스트**
```ts
it('각 자식 SidechainGroup.conclusion = 그 agent의 마지막 assistant_message', () => {
  const evs = [
    asstMain('m1','병렬'), taskCall('m1','tcA','tuA'), sidecar('A','tuA','Explore','조사 A'),
    scUser('A','pA'), scAsst('A','a1','중간'), scAsst('A','a2','최종 결론입니다'),
  ];
  const items = buildStreamModel(evs);
  // N=1 → 배치 래핑 없음, 단일 SidechainGroup
  const g = items.find((i) => i.type === 'sidechain-group');
  expect(g.conclusion).toBe('최종 결론입니다');
});
```

- [ ] **Step 3: 실패 확인** — Run: `npx vitest run ...buildStreamModel... -t '마지막 assistant_message'` · Expected: FAIL(`conclusion` undefined).

- [ ] **Step 4: 결론 추출 구현** — `closeGroup()` 내부에서 scBuf의 message 항목 중 role==='assistant' 마지막의 text를 잘라 conclusion에 넣는다. `closeGroup`에서 group push 시:
```ts
let conclusion: string | null = null;
for (const it of scBuf) {
  if (it.type === 'message' && it.role === 'assistant' && it.text.trim()) {
    conclusion = it.text.trim().slice(0, 200);
  }
}
```
그리고 push되는 객체에 `conclusion,` 추가.

- [ ] **Step 5: 통과 확인** — Run 동일 · Expected: PASS.

- [ ] **Step 6: 커밋**
```bash
git add -A && git commit -m "feat(webui): SidechainGroup 결론 추출(마지막 assistant_message)"
```

---

## Task 3: agent_id 전역 수집(de-interleave) — 조각화 제거

**Files:** Modify `streamModel.ts`

**핵심 변경:** 현재 `emit`은 agent_id가 바뀌면 `closeGroup()`로 끊는다(조각화). 이를 **agent_id별 버퍼 맵**으로 바꿔, 메인 복귀(또는 종료) 시 전 버퍼를 한꺼번에 flush한다.

- [ ] **Step 1: 실패 테스트**
```ts
it('교차 도착해도 한 agent는 한 SidechainGroup으로(조각 안 남)', () => {
  const evs = [
    asstMain('m1','병렬'), taskCall('m1','tcA','tuA'), taskCall('m1','tcB','tuB'),
    sidecar('A','tuA','Explore','A'), sidecar('B','tuB','general','B'),
    scAsst('A','a1','A1'), scAsst('B','b1','B1'), scAsst('A','a2','A2'), scAsst('B','b2','B2'),
  ];
  const items = buildStreamModel(evs);
  const groups = collectSidechainGroups(items); // 헬퍼: batch 안/밖 모든 sidechain-group
  const byAgent = groups.filter((g) => g.agentId === 'A');
  expect(byAgent).toHaveLength(1); // 조각 X
  expect(byAgent[0].items.filter((i) => i.type === 'message')).toHaveLength(2);
});
```
헬퍼:
```ts
function collectSidechainGroups(items) {
  const out = [];
  for (const it of items) {
    if (it.type === 'sidechain-group') out.push(it);
    if (it.type === 'batch-group') out.push(...it.agentGroups);
  }
  return out;
}
```

- [ ] **Step 2: 실패 확인** — Run: `-t '조각 안 남'` · Expected: FAIL(현재 2조각).

- [ ] **Step 3: 전역 수집 구현** — `scBuf:StreamItem[]|null` + `scAgent` 단일 버퍼를 **`scBufs: Map<string, StreamItem[]>`** + 순서 보존 `scOrder: string[]`로 교체. `emit(sidechain)`은 agent_id 키 버퍼에 append(끊지 않음). `closeGroup`(이름 `flushSidechain`로)은 `scOrder` 순서대로 각 버퍼를 SidechainGroup으로 push 후 맵 clear. agent_id 없는(null) 이벤트는 직전 agent 키에 append(contiguity fallback) — `lastScAgent` 기억.

```ts
const scBufs = new Map<string, StreamItem[]>();
const scOrder: string[] = [];
let lastScAgent: string | null = null;
const flushSidechain = () => {
  for (const key of scOrder) {
    const buf = scBufs.get(key); if (!buf || !buf.length) continue;
    const agentId = key === '∅' ? null : key;
    // ... meta/taskEventId/conclusion 계산(Task 2) ... push sidechain-group
  }
  scBufs.clear(); scOrder.length = 0; lastScAgent = null;
};
const emitSidechain = (it: StreamItem, agentId: string | null) => {
  const key = agentId ?? lastScAgent ?? '∅';
  if (!scBufs.has(key)) { scBufs.set(key, []); scOrder.push(key); }
  scBufs.get(key)!.push(it);
  if (agentId) lastScAgent = agentId;
};
```
`emit`에서 sidechain 분기를 `emitSidechain`로, 비-sidechain 분기는 `flushSidechain()` 후 main push. 마지막 `closeGroup()` 호출도 `flushSidechain()`로.

- [ ] **Step 4: 통과 확인** — Run: `-t '조각 안 남'` + 기존 sidechain 테스트 전체 · Expected: PASS(기존 단일-agent 테스트 회귀 없음).

- [ ] **Step 5: 커밋**
```bash
git add -A && git commit -m "feat(webui): sidechain을 agent_id로 전역 수집(병렬 조각화 제거)"
```

---

## Task 4: 형제를 BatchGroup으로 래핑 + 종합 + 단일/ degrade

**Files:** Modify `streamModel.ts`

- [ ] **Step 1: 실패 테스트(래핑·종합·단일)**
```ts
it('같은 message_id 디스패치 형제는 한 BatchGroup, 종합=배치 후 main 메시지', () => {
  const evs = [
    asstMain('m1','병렬'), taskCall('m1','tcA','tuA'), taskCall('m1','tcB','tuB'),
    sidecar('A','tuA','Explore','A'), sidecar('B','tuB','general','B'),
    scAsst('A','a1','A결론'), scAsst('B','b1','B결론'),
    asstMain('m2','두 결과 종합하면 X'),
  ];
  const items = buildStreamModel(evs);
  const batch = items.find((i) => i.type === 'batch-group');
  expect(batch.agentGroups).toHaveLength(2);
  expect(batch.synthesis).toContain('종합하면');
  expect(batch.settled).toBe(true);
});
it('단일 디스패치는 BatchGroup 없이 SidechainGroup', () => {
  const evs = [asstMain('m1','하나'), taskCall('m1','tcA','tuA'), sidecar('A','tuA','Explore','A'), scAsst('A','a1','끝')];
  const items = buildStreamModel(evs);
  expect(items.some((i) => i.type === 'batch-group')).toBe(false);
  expect(items.some((i) => i.type === 'sidechain-group')).toBe(true);
});
```

- [ ] **Step 2: 실패 확인** — Run: `-t 'BatchGroup'` · Expected: FAIL.

- [ ] **Step 3: 래핑 구현** — `flushSidechain`이 SidechainGroup들을 곧장 `items`에 넣는 대신 **임시 배열로 모으고**, flush 시점에 그 형제들을 **디스패치 message_id로 그룹핑**한다. 매핑: prepass에서 `callMsgByUse: Map<tool_use_id, message_id>` 추가(`if (e.kind==='tool_call' && e.tool_use_id) callMsgByUse.set(e.tool_use_id, e.message_id ?? null)`). 각 SidechainGroup의 dispatch message_id = `metaByAgent.get(agent)?.toolUseId` → `callMsgByUse`. 같은 message_id가 2개 이상이면 `BatchGroup`(settled = 모든 자식 conclusion!=null), 1개면 그대로 SidechainGroup push.
```ts
// flush 시: groups: SidechainGroup[] 생성 후
const byMsg = new Map<string, SidechainGroup[]>();
for (const g of groups) {
  const tu = g.agentId ? metaByAgent.get(g.agentId)?.toolUseId : null;
  const mid = tu ? callMsgByUse.get(tu) ?? null : null;
  const key = mid ?? `solo-${g.id}`;
  (byMsg.get(key) ?? byMsg.set(key, []).get(key)!).push(g);
}
for (const [, sibs] of byMsg) {
  if (sibs.length >= 2) items.push({ type:'batch-group', id:`batch-${sibs[0].id}`, agentGroups: sibs, synthesis: null, settled: sibs.every((s)=>s.conclusion!=null) });
  else items.push(sibs[0]);
}
```
종합(synthesis): flush는 메인 복귀 시 일어나므로, flush 직후 첫 main assistant_message를 만나면 직전 BatchGroup의 `synthesis`에 채운다 — main message emit 직전 `pendingBatchForSynthesis` 참조를 두고 첫 assistant main이면 `.text.slice(0,200)` 대입.

- [ ] **Step 4: 통과 확인** — Run: `-t 'BatchGroup'` + 단일 테스트 · Expected: PASS.

- [ ] **Step 5: degrade 테스트 + 확인**
```ts
it('agent_id 없는 sidechain은 contiguity로 묶이고 배치 미형성', () => {
  const evs = [scAsst('', 'x1' as any, 'pre0023')]; // agent_id '' 
  // 위 헬퍼 scAsst의 agent_id를 ''로: base({... agent_id:'' ...})
  const items = buildStreamModel(evs);
  expect(items.some((i)=>i.type==='batch-group')).toBe(false);
});
```
Run 후 PASS 확인.

- [ ] **Step 6: 커밋**
```bash
git add -A && git commit -m "feat(webui): 형제 서브에이전트를 BatchGroup으로 래핑 + 종합 결과"
```

---

## Task 5: BatchGroup 컴포넌트 (L0/L1)

**Files:** Create `BatchGroup.tsx`, `BatchGroup.module.css`; Test `__tests__/BatchGroup.test.tsx`

- [ ] **Step 1: 실패 테스트**
```tsx
it('L0 접힘: 배치 식별 + 종합 결과 보임, 자식 숨김', () => {
  render(<BatchGroup group={fixtureBatch} selectedEventId={null} onSelect={()=>{}} findingEventIds={new Set()} />);
  expect(screen.getByTestId('batch-group')).toHaveAttribute('data-expanded','false');
  expect(screen.getByTestId('batch-synthesis')).toHaveTextContent('종합');
  expect(screen.queryByTestId('subagent-group')).toBeNull();
});
it('펼치면 자식 SubagentGroup 들 보임', () => {
  render(<BatchGroup .../>);
  fireEvent.click(screen.getByTestId('batch-toggle'));
  expect(screen.getAllByTestId('subagent-group').length).toBe(fixtureBatch.agentGroups.length);
});
```
(`fixtureBatch`: type:'batch-group', agentGroups 2개(각 conclusion 보유), synthesis '…종합…', settled true.)

- [ ] **Step 2: 실패 확인** — Run: `npx vitest run src/components/replay/stream/__tests__/BatchGroup.test.tsx` · Expected: FAIL(모듈 없음).

- [ ] **Step 3: 컴포넌트 구현** — `SubagentGroup` 패턴을 따른다(useState userOverride, containsSelected 자동 펼침). 헤더: 칩 "병렬 배치", `agentGroups.length` agents, 상태(settled? ✓ N/N : ⏳), 총 소요(자식 min/max), toggle. 접힘 시 `data-testid="batch-synthesis"`로 종합 줄(없으면 진행 중). 펼침 시 `agentGroups.map((g)=> <SubagentGroup group={g} ... />)` + outcome 줄. 기본 접힘(`userOverride ?? containsSelected ?? false`); settled=false면 기본 펼침(보조).

- [ ] **Step 4: 통과 확인** — Run 동일 · Expected: PASS.

- [ ] **Step 5: 커밋**
```bash
git add -A && git commit -m "feat(webui): BatchGroup 컴포넌트(L0 배치+종합 / L1 자식)"
```

---

## Task 6: SubagentGroup — 결론 줄 + 기본 접힘

**Files:** Modify `SubagentGroup.tsx`, `SubagentGroup.module.css`, `__tests__/SubagentGroup.test.tsx`

- [ ] **Step 1: 실패 테스트**
```tsx
it('접힘 축약 줄에 결론 보임', () => {
  render(<SubagentGroup group={{...fixtureGroup, conclusion:'핵심 결론'}} .../>);
  expect(screen.getByTestId('subagent-conclusion')).toHaveTextContent('핵심 결론');
});
```

- [ ] **Step 2: 실패 확인** — Run: `-t '결론 보임'` · Expected: FAIL.

- [ ] **Step 3: 구현** — 헤더 아래(접힘에서도 보이는 위치)에 `group.conclusion &&`일 때 `<div data-testid="subagent-conclusion" className={styles.conclusion}>결론 · {group.conclusion}</div>`. css `.conclusion` 추가(fg-muted, 작은 글씨, b 강조 #8fe6c8).

- [ ] **Step 4: 통과 확인** — Run + 기존 SubagentGroup 테스트 전체 · Expected: PASS.

- [ ] **Step 5: 커밋**
```bash
git add -A && git commit -m "feat(webui): SubagentGroup 축약 줄 결론 표시"
```

---

## Task 7: ConversationStream — batch-group 렌더·스크롤 타깃

**Files:** Modify `ConversationStream.tsx`, `__tests__/ConversationStream.test.tsx`

- [ ] **Step 1: 실패 테스트** — items에 batch-group 1개 넣고 `renderItem`이 BatchGroup을 렌더하는지 / `itemContainsEvent`가 배치 자식 이벤트를 찾는지.
```tsx
it('batch-group 렌더 + 자식 이벤트 포함 판정', () => {
  render(<ConversationStream items={[fixtureBatchItem]} selectedEventId={'a2'} .../>);
  expect(screen.getByTestId('batch-group')).toBeInTheDocument();
});
```

- [ ] **Step 2: 실패 확인** — Run: ConversationStream 테스트 · Expected: FAIL.

- [ ] **Step 3: 구현** — `import { BatchGroup } from './BatchGroup'`. `renderItem`에 분기:
```tsx
if (item.type === 'batch-group') {
  return <BatchGroup group={item} selectedEventId={selectedEventId} onSelect={onSelect} findingEventIds={findingEventIds} />;
}
```
`itemContainsEvent`에 추가: `if (item.type === 'batch-group') return item.agentGroups.some((g) => itemContainsEvent(g, eventId));` (SidechainGroup 분기 재사용 위해 `g`를 sidechain-group으로 넘김).

- [ ] **Step 4: 통과 확인** — Run · Expected: PASS.

- [ ] **Step 5: 전체 테스트 + 커밋**
```bash
cd webui && npx vitest run && cd .. && git add -A && git commit -m "feat(webui): ConversationStream에 BatchGroup 렌더·스크롤 타깃"
```

---

## Task 8: 브라우저 smoke (CLAUDE.md 의무)

**Files:** 없음(검증 전용)

- [ ] **Step 1:** `cd webui && npm run build`로 dist 갱신(또는 vite dev :5173).
- [ ] **Step 2:** `wimcc serve` 기동 후, 병렬 배치가 있는 세션(`fb6b8e3a…`)을 브라우저로 열어 확인:
  - L0 접힘에 배치+종합 보임, L1 펼침에 5개 요약(결론 포함), L2 펼침에 상세.
  - 한 agent가 한 블록으로(조각 X), Task 점프 정상.
  - claude-in-chrome 도구로 스크린샷 시각 검증.
- [ ] **Step 3:** 이상 없으면 완료. (재ingest 불필요 — 순수 렌더 변경.)

---

## Self-Review (작성자 체크)

- **Spec coverage:** §3.1 전역 수집=Task3 · §3.2 스트리밍(점진/settled)=Task4(settled)+컴포넌트 진행상태 · §3.3 결론=Task2/종합=Task4 · §3.4 L0/L1/L2=Task5/6 · §6 degrade=Task4 Step5 · §8 테스트=각 Task. **갭: §3.2 "분류 중" 버킷(미귀속 라이브 이벤트)은 Q3 미해결 → 본 plan 범위 외(후속).** §6 "단일 디스패치 no-wrapper"=Task4. 
- **Placeholder scan:** 코드 스텝에 실제 코드 포함. 컴포넌트 css 세부는 토큰 재사용으로 명시.
- **Type consistency:** `BatchGroup{type,id,agentGroups,synthesis,settled}` · `SidechainGroup.conclusion` — Task1 정의가 Task4/5/7에서 동일 사용. `flushSidechain`(구 closeGroup) 이름 일관.

## 주의(실측 기반)

- 표본 1 세션 기반 — message_id 기반 배치 묶음이 다른 디스패치 패턴(예: 여러 턴에 걸친 디스패치)에서 어떻게 보이는지는 추가 세션으로 확인 필요. 일반화 단정 금지.
- 스트리밍 라이브 라우팅의 정확 귀속은 사이드카(agent_id↔tool_use_id) 도착 타이밍 의존 — 라이브 환경(`wimcc serve` + 현재 세션) smoke로 별도 확인 권장.
