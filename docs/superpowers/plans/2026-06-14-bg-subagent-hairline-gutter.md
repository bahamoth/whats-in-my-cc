# Background-Subagent Hairline Gutter Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Visualize a standalone **background** subagent (Agent tool `run_in_background`) that gets fragmented by interleaving main messages/tool-calls — render a fixed-width (~14px) left "hairline gutter" whose continuous per-agent colored rail spans the rows that occurred during the subagent's run, with start (▢) / end (✓) nodes, so its background presence is legible without widening the layout or reordering the honest time axis.

**Architecture:** Pure **webui** change (no backend/migration — `agent_id` is already on the DTO since migration 0023, timestamps drive the span). A new deterministic `agentColor(agent_id)` hash → stable palette is the single source of color truth shared by the gutter rail, the start/end glyphs, and the SubagentGroup block header (swatch + id) which stays the accessible identity surface. A new pure function `computeBgGutter(items)` derives, per stream row, the lane cells (≤3 lanes packed at a 4px pitch; >3 → dense gray spine + count). `ConversationStream` wraps each virtual row in a flex `[gutter][body]` so row **height** is unchanged (the virtualizer only depends on height) — no reflow risk.

**Tech Stack:** React + TypeScript + Vite, CSS modules, `@tanstack/react-virtual`, vitest + @testing-library.

**Continues branch:** `feat/workflow-grouping` (same background-subagent-visibility epic — do NOT open a new branch). Smoke before commit; PR at the end.

**Approved design artifact:** `webui/public/wf-bg-hairline-proto.html` (user-approved compact gutter, terminology "백그라운드" not "동시").

---

## Real-data anchoring (locks)

- `agent_id` on `ObservedEventDto` — PRESENT (migration 0023, frozen fixture `subagent_sidecar_v01`). The gutter's agent identity.
- Subagent **span** = `[min,max]` of the group's event timestamps (same derivation `SubagentGroup.summarizeGroup` already uses). No `subagent_completed`/`duration_ms` event exists — **must NOT claim it drives the UI**; the ✓ end node is the span end and any duration is labelled derived.
- No per-agent-per-row membership exists today (only a per-message COUNT `concurrentBackground`). This plan adds the per-row agent SET as a separate pure function — it does not change the existing count field.
- Scope of lanes = **top-level `sidechain-group` items only** (standalone background subagents). `batch-group`/`workflow-group` are containers with their own viz and are NOT the problem per the user; they contribute no lanes (but their rows still receive gutter coverage if a standalone agent overlaps them).

## File Structure

- Create: `webui/src/lib/colorHash.ts` — `agentColor(agentId)` deterministic hash→palette (single source of color truth).
- Create: `webui/src/lib/__tests__/colorHash.test.ts`.
- Modify: `webui/src/components/replay/stream/streamModel.ts` — add `GutterCell`/`GutterRow` types + `computeBgGutter(items)`; no change to `annotateConcurrency`.
- Modify: `webui/src/components/replay/stream/__tests__/buildStreamModel.test.ts` — add `computeBgGutter` cases.
- Create: `webui/src/components/replay/stream/BgGutter.tsx` + `BgGutter.module.css` — the per-row gutter renderer.
- Create: `webui/src/components/replay/stream/__tests__/BgGutter.test.tsx`.
- Modify: `webui/src/components/replay/stream/ConversationStream.tsx` — `useMemo(computeBgGutter)`, wrap each row (virtual + fallback) in `[BgGutter][body]`.
- Modify: `webui/src/components/replay/stream/ConversationStream.module.css` — `.row`/`.rowBody` flex.
- Modify: `webui/src/components/replay/stream/SubagentGroup.tsx` + `.module.css` — per-agent color via inline `--agentColor`, add color swatch (identity surface).
- Modify: `webui/src/components/replay/stream/__tests__/SubagentGroup.test.tsx` (or ConversationStream test) — swatch + color var.
- Modify: `webui/src/components/replay/stream/MessageCard.tsx:171` + `MessageCard.test.tsx:26` — wording 동시→백그라운드.
- Modify: `docs/implementation-notes.html` — record the gutter design + data caveat.

---

### Task 1: `agentColor` deterministic hash → palette

**Files:**
- Create: `webui/src/lib/colorHash.ts`
- Test: `webui/src/lib/__tests__/colorHash.test.ts`

- [ ] **Step 1: Write the failing test**

```ts
// webui/src/lib/__tests__/colorHash.test.ts
import { describe, it, expect } from 'vitest';
import { agentColor, AGENT_PALETTE } from '../colorHash';

describe('agentColor', () => {
  it('is deterministic: same id → same color', () => {
    expect(agentColor('aa1844')).toBe(agentColor('aa1844'));
  });
  it('maps into the fixed palette', () => {
    expect(AGENT_PALETTE).toContain(agentColor('aa1844'));
    expect(AGENT_PALETTE.length).toBeGreaterThanOrEqual(6);
  });
  it('different ids generally differ (no single-bucket collapse)', () => {
    const ids = ['aa1844', '7b20e4', 'c4d8a0', 'deadbe', '012345', 'fffabc'];
    const colors = new Set(ids.map(agentColor));
    expect(colors.size).toBeGreaterThanOrEqual(4);
  });
  it('null / empty → neutral subtle var (not a palette hue)', () => {
    expect(agentColor(null)).toBe('var(--wimcc-fg-subtle, #6a7180)');
    expect(agentColor('')).toBe('var(--wimcc-fg-subtle, #6a7180)');
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd webui && npx vitest run src/lib/__tests__/colorHash.test.ts`
Expected: FAIL (module not found / agentColor undefined).

- [ ] **Step 3: Write minimal implementation**

```ts
// webui/src/lib/colorHash.ts
// Deterministic per-agent color = hash(agent_id) → stable palette. The SINGLE
// source of color truth for background-subagent identity: the hairline gutter
// rail, its ▢/✓ glyphs, and the SubagentGroup block header swatch all read this
// for the same agent so the color ties block ↔ gutter together. Distinct from
// the SEMANTIC --wimcc-lane-* tokens (those mean event kind, not identity).
//
// Tens of agents WILL collide across this fixed palette — that is accepted: the
// gutter is a calm presence indicator, and the authoritative re-confirmable
// identity is the block header (swatch + agent_id text), not the hue.

/** 8 readable hues (dark-bg first; mid-tones stay legible in light mode). */
export const AGENT_PALETTE = [
  '#7da7ff', // blue
  '#41c285', // green
  '#d97aff', // violet
  '#ff8a4c', // orange
  '#2bd0d0', // teal
  '#f0b429', // amber
  '#ef6f9c', // pink
  '#9d8bff', // periwinkle
] as const;

const NEUTRAL = 'var(--wimcc-fg-subtle, #6a7180)';

/** Stable color for an agent. null/'' (pre-0023 ingests / non-subagent) → neutral. */
export function agentColor(agentId: string | null | undefined): string {
  if (!agentId) return NEUTRAL;
  let h = 0;
  for (let i = 0; i < agentId.length; i++) h = (h * 31 + agentId.charCodeAt(i)) >>> 0;
  return AGENT_PALETTE[h % AGENT_PALETTE.length];
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd webui && npx vitest run src/lib/__tests__/colorHash.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add webui/src/lib/colorHash.ts webui/src/lib/__tests__/colorHash.test.ts
git commit -m "feat(webui): agentColor — 결정론 hash(agent_id)→안정 팔레트(가시화 색 단일 출처)"
```

---

### Task 2: `computeBgGutter` — per-row lane cells from standalone background subagents

**Files:**
- Modify: `webui/src/components/replay/stream/streamModel.ts` (add types near other exports; add function after `annotateConcurrency`)
- Test: `webui/src/components/replay/stream/__tests__/buildStreamModel.test.ts`

Design: lanes are packed greedily (interval partitioning, cap 3) so each agent keeps ONE x-slot for its whole life (stable, no flicker); an agent that cannot get a slot 0–2 (≥4 simultaneous) is "overflow". A row covered by any overflow agent → **dense** (single neutral spine + count). Markers: an agent's own block row = `start`; its last covered row = `end`; covered rows in between = `mid`.

- [ ] **Step 1: Write the failing test** (append to `buildStreamModel.test.ts`)

```ts
import { computeBgGutter } from '../streamModel';
import type { StreamItem, SidechainGroup, MessageItem } from '../streamModel';

// minimal builders
const msg = (id: string, ts: string, over: Partial<MessageItem> = {}): MessageItem =>
  ({ type: 'message', id, eventId: id, role: 'assistant', model: null, text: id, timestamp: ts, sidechain: false, ...over });
const subMsg = (id: string, ts: string): MessageItem =>
  ({ type: 'message', id, eventId: id, role: 'assistant', model: null, text: id, timestamp: ts, sidechain: true });
const agent = (id: string, agentId: string, tss: string[]): SidechainGroup =>
  ({ type: 'sidechain-group', id, agentId, agentType: null, description: null, taskEventId: null,
     conclusion: null, items: tss.map((t, i) => subMsg(`${id}-e${i}`, t)) });

describe('computeBgGutter', () => {
  it('single bg agent: start on its block, mid on interleaved mains, end on last covered', () => {
    const A = agent('A', 'aa1844', ['2026-06-14T01:41:05Z', '2026-06-14T01:42:24Z']);
    const items: StreamItem[] = [
      A,
      msg('m1', '2026-06-14T01:41:12Z'),
      msg('m2', '2026-06-14T01:41:58Z'),
      msg('m3', '2026-06-14T01:50:00Z'), // after A's span → no cell
    ];
    const g = computeBgGutter(items);
    expect(g.get('A')!.cells[0]).toMatchObject({ lane: 0, agentId: 'aa1844', marker: 'start' });
    expect(g.get('m1')!.cells[0]).toMatchObject({ lane: 0, marker: 'mid' });
    expect(g.get('m2')!.cells[0]).toMatchObject({ lane: 0, marker: 'end' }); // last in-span row
    expect(g.get('m3')).toBeUndefined();
  });

  it('three concurrent bg agents pack into lanes 0,1,2 (gutter width constant)', () => {
    const A = agent('A', 'a', ['2026-06-14T01:00:00Z', '2026-06-14T01:10:00Z']);
    const B = agent('B', 'b', ['2026-06-14T01:01:00Z', '2026-06-14T01:09:00Z']);
    const C = agent('C', 'c', ['2026-06-14T01:02:00Z', '2026-06-14T01:08:00Z']);
    const items: StreamItem[] = [A, B, C, msg('m', '2026-06-14T01:05:00Z')];
    const cells = computeBgGutter(items).get('m')!.cells;
    expect(cells.map((c) => c.lane).sort()).toEqual([0, 1, 2]);
    expect(new Set(cells.map((c) => c.agentId)).size).toBe(3);
    expect(computeBgGutter(items).get('m')!.dense).toBe(0);
  });

  it('four concurrent → dense (count, no per-lane cells) for the overflow-covered row', () => {
    const mk = (k: string) => agent(k, k, ['2026-06-14T01:00:00Z', '2026-06-14T01:10:00Z']);
    const items: StreamItem[] = [mk('a'), mk('b'), mk('c'), mk('d'), msg('m', '2026-06-14T01:05:00Z')];
    const row = computeBgGutter(items).get('m')!;
    expect(row.dense).toBe(4);
  });

  it('no bg agents → empty map', () => {
    expect(computeBgGutter([msg('m', '2026-06-14T01:00:00Z')]).size).toBe(0);
  });

  it('agentId null → no lane contributed (graceful)', () => {
    const A = { ...agent('A', 'x', ['2026-06-14T01:00:00Z', '2026-06-14T01:05:00Z']), agentId: null };
    const items: StreamItem[] = [A, msg('m', '2026-06-14T01:02:00Z')];
    expect(computeBgGutter(items).size).toBe(0);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd webui && npx vitest run src/components/replay/stream/__tests__/buildStreamModel.test.ts -t computeBgGutter`
Expected: FAIL (`computeBgGutter` is not exported).

- [ ] **Step 3: Write minimal implementation** (add to `streamModel.ts`)

Add types after the `StreamItem` union (~line 200):

```ts
/** One lane cell painted in the background-subagent gutter for a given row. */
export interface GutterCell {
  /** x-slot 0..2 (stable per agent for its whole life). */
  lane: number;
  agentId: string;
  /** hash(agent_id) color — shared with the SubagentGroup block header. */
  color: string;
  marker: 'start' | 'mid' | 'end';
}
/** Per-row gutter descriptor. `dense>0` ⇒ collapse to one neutral spine + count
 *  (≥4 background subagents overlap this row); otherwise render `cells`. */
export interface GutterRow {
  cells: GutterCell[];
  dense: number;
}
```

Add the function (after `annotateConcurrency`, importing the helper at top: `import { agentColor } from '../../../lib/colorHash';`):

```ts
const MAX_LANES = 3;

/** Representative wall-clock (ms) of a top-level row for gutter coverage tests. */
function rowTimeMs(it: StreamItem): number | null {
  const t = (iso: string) => {
    const n = new Date(iso).getTime();
    return Number.isNaN(n) ? null : n;
  };
  if (it.type === 'message') return t(it.timestamp);
  if (it.type === 'activity-run') return it.events.length ? t(it.events[0].event.observed_at) : null;
  if (it.type === 'thinking') return it.events.length ? t(it.events[0].timestamp) : null;
  if (it.type === 'scaffold-group') return it.items.length ? t(it.items[0].timestamp) : null;
  // group containers: earliest child event
  const groups = it.type === 'sidechain-group' ? [it] : it.agentGroups;
  let min = Infinity;
  for (const g of groups) for (const c of g.items) {
    if (c.type === 'message') { const x = t(c.timestamp); if (x != null) min = Math.min(min, x); }
    else if (c.type === 'activity-run') for (const ae of c.events) { const x = t(ae.event.observed_at); if (x != null) min = Math.min(min, x); }
  }
  return Number.isFinite(min) ? min : null;
}

function sidechainSpan(g: SidechainGroup): { s: number; e: number } | null {
  let s = Infinity, e = -Infinity;
  const see = (iso: string) => { const n = new Date(iso).getTime(); if (!Number.isNaN(n)) { s = Math.min(s, n); e = Math.max(e, n); } };
  for (const it of g.items) {
    if (it.type === 'message') see(it.timestamp);
    else if (it.type === 'activity-run') for (const ae of it.events) see(ae.event.observed_at);
    else if (it.type === 'thinking') for (const ev of it.events) see(ev.timestamp);
  }
  return e > s ? { s, e } : null;
}

/** Per-row background-subagent gutter. Lanes come ONLY from TOP-LEVEL
 *  sidechain-groups (standalone background subagents); batch/workflow containers
 *  have their own viz and contribute no lanes. Greedy interval partitioning caps
 *  at 3 lanes (stable x per agent for its whole life); a row covered by an agent
 *  that could not get a lane (≥4 simultaneous) is `dense`. */
export function computeBgGutter(items: StreamItem[]): Map<string, GutterRow> {
  type Ag = { agentId: string; blockId: string; s: number; e: number; lane: number; endRowId: string | null };
  const agents: Ag[] = [];
  for (const it of items) {
    if (it.type !== 'sidechain-group' || !it.agentId) continue;
    const sp = sidechainSpan(it);
    if (sp) agents.push({ agentId: it.agentId, blockId: it.id, s: sp.s, e: sp.e, lane: -1, endRowId: null });
  }
  const out = new Map<string, GutterRow>();
  if (!agents.length) return out;

  // greedy lane assignment (stable per agent): sort by start, lowest free lane.
  agents.sort((a, b) => a.s - b.s);
  const laneFreeAt = new Array(MAX_LANES).fill(-Infinity);
  for (const a of agents) {
    for (let L = 0; L < MAX_LANES; L++) {
      if (a.s >= laneFreeAt[L]) { a.lane = L; laneFreeAt[L] = a.e; break; }
    }
  }

  // last covered row per agent (for the ✓ end marker): walk rows in order.
  for (const a of agents) {
    let last: string | null = null;
    for (const it of items) {
      const t = rowTimeMs(it);
      if (t != null && t >= a.s && t <= a.e) last = it.id;
    }
    a.endRowId = last;
  }

  for (const it of items) {
    const t = rowTimeMs(it);
    if (t == null) continue;
    const covering = agents.filter((a) => t >= a.s && t <= a.e);
    if (!covering.length) continue;
    const overflow = covering.some((a) => a.lane < 0);
    if (overflow) {
      out.set(it.id, { cells: [], dense: covering.length });
      continue;
    }
    const cells: GutterCell[] = covering.map((a) => ({
      lane: a.lane,
      agentId: a.agentId,
      color: agentColor(a.agentId),
      marker: a.blockId === it.id ? 'start' : a.endRowId === it.id ? 'end' : 'mid',
    }));
    cells.sort((x, y) => x.lane - y.lane);
    out.set(it.id, { cells, dense: 0 });
  }
  return out;
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd webui && npx vitest run src/components/replay/stream/__tests__/buildStreamModel.test.ts -t computeBgGutter`
Expected: PASS (all 5 cases).

- [ ] **Step 5: Commit**

```bash
git add webui/src/components/replay/stream/streamModel.ts webui/src/components/replay/stream/__tests__/buildStreamModel.test.ts
git commit -m "feat(webui): computeBgGutter — 단독 백그라운드 서브에이전트의 per-row 레인(≤3 팩킹, >3 dense)"
```

---

### Task 3: MessageCard bg-marker wording 동시 → 백그라운드

**Files:**
- Modify: `webui/src/components/replay/stream/MessageCard.tsx:171` (+ title attr ~169)
- Test: `webui/src/components/replay/stream/__tests__/MessageCard.test.tsx:26`

- [ ] **Step 1: Update the failing test** (change the existing expectation at line ~26)

```ts
    expect(screen.getByTestId('bg-marker')).toHaveTextContent('백그라운드 2개 실행 중');
```
Also update the test name string on line ~18 to read `"백그라운드 N개 실행 중"`.

- [ ] **Step 2: Run to verify it fails**

Run: `cd webui && npx vitest run src/components/replay/stream/__tests__/MessageCard.test.tsx -t bg-marker`
Expected: FAIL (still renders old "동시 실행" text).

- [ ] **Step 3: Implement** — in `MessageCard.tsx` replace the marker text + title:

```tsx
            title={`이 메시지가 진행되는 동안 백그라운드 서브에이전트 ${item.concurrentBackground}개가 실행 중이었음 (이 메시지가 백그라운드라는 뜻이 아님)`}
          >
            ⟂ 백그라운드 {item.concurrentBackground}개 실행 중
```

- [ ] **Step 4: Run to verify it passes**

Run: `cd webui && npx vitest run src/components/replay/stream/__tests__/MessageCard.test.tsx`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add webui/src/components/replay/stream/MessageCard.tsx webui/src/components/replay/stream/__tests__/MessageCard.test.tsx
git commit -m "fix(webui): bg-marker 용어 '동시'→'백그라운드'(Claude 어휘 일치)"
```

---

### Task 4: SubagentGroup per-agent color + identity swatch

**Files:**
- Modify: `webui/src/components/replay/stream/SubagentGroup.tsx`
- Modify: `webui/src/components/replay/stream/SubagentGroup.module.css`
- Test: `webui/src/components/replay/stream/__tests__/SubagentGroup.test.tsx`

- [ ] **Step 1: Write the failing test** (create file if absent)

```tsx
import { render, screen } from '@testing-library/react';
import { describe, it, expect } from 'vitest';
import { SubagentGroup } from '../SubagentGroup';
import type { SidechainGroup } from '../streamModel';
import { agentColor } from '../../../../lib/colorHash';

const g = (over: Partial<SidechainGroup> = {}): SidechainGroup => ({
  type: 'sidechain-group', id: 'A', agentId: 'aa1844', agentType: 'general-purpose',
  description: 'Task 3', taskEventId: null, conclusion: null, items: [], ...over,
});

describe('SubagentGroup color identity', () => {
  it('sets --agentColor from hash(agentId) and shows a color swatch', () => {
    render(<SubagentGroup group={g()} selectedEventId={null} onSelect={() => {}} findingEventIds={new Set()} />);
    const section = screen.getByTestId('subagent-group');
    expect(section.style.getPropertyValue('--agentColor')).toBe(agentColor('aa1844'));
    expect(screen.getByTestId('subagent-swatch')).toBeInTheDocument();
  });
  it('agentId null → neutral, swatch still present (graceful)', () => {
    render(<SubagentGroup group={g({ agentId: null })} selectedEventId={null} onSelect={() => {}} findingEventIds={new Set()} />);
    expect(screen.getByTestId('subagent-group').style.getPropertyValue('--agentColor')).toBe(agentColor(null));
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd webui && npx vitest run src/components/replay/stream/__tests__/SubagentGroup.test.tsx`
Expected: FAIL (no --agentColor / no swatch).

- [ ] **Step 3: Implement**

In `SubagentGroup.tsx`: import the helper and set the inline var + swatch.

```tsx
import { agentColor } from '../../../lib/colorHash';
// ...
  return (
    <section
      data-testid="subagent-group"
      data-expanded={String(expanded)}
      className={styles.group}
      style={{ ['--agentColor' as string]: agentColor(group.agentId) }}
    >
```

Add the swatch right after the `Subagent` label span (after line ~106):

```tsx
          <span className={styles.label}>Subagent</span>
          <span data-testid="subagent-swatch" className={styles.swatch} aria-hidden />
```

In `SubagentGroup.module.css`: swap the four hardcoded `--wimcc-lane-context` to the per-agent var (keep violet as fallback) and add `.swatch`.

```css
.group { border-left: 2px solid var(--agentColor, var(--wimcc-lane-context, #b07dff)); }
.header { color: var(--agentColor, var(--wimcc-lane-context, #b07dff)); }
.agentType { color: var(--agentColor, var(--wimcc-lane-context, #b07dff)); }
.agentChip { color: var(--agentColor, var(--wimcc-lane-context, #b07dff)); }

.swatch {
  flex: none;
  width: 9px;
  height: 9px;
  border-radius: 2px;
  background: var(--agentColor, var(--wimcc-lane-context, #b07dff));
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `cd webui && npx vitest run src/components/replay/stream/__tests__/SubagentGroup.test.tsx`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add webui/src/components/replay/stream/SubagentGroup.tsx webui/src/components/replay/stream/SubagentGroup.module.css webui/src/components/replay/stream/__tests__/SubagentGroup.test.tsx
git commit -m "feat(webui): SubagentGroup 인스턴스 색(--agentColor)+신원 스와치(violet 하드코딩 대체)"
```

---

### Task 5: `BgGutter` component + ConversationStream per-row integration

**Files:**
- Create: `webui/src/components/replay/stream/BgGutter.tsx`
- Create: `webui/src/components/replay/stream/BgGutter.module.css`
- Test: `webui/src/components/replay/stream/__tests__/BgGutter.test.tsx`
- Modify: `webui/src/components/replay/stream/ConversationStream.tsx`
- Modify: `webui/src/components/replay/stream/ConversationStream.module.css`

- [ ] **Step 1: Write the failing test**

```tsx
// BgGutter.test.tsx
import { render, screen } from '@testing-library/react';
import { describe, it, expect } from 'vitest';
import { BgGutter } from '../BgGutter';

describe('BgGutter', () => {
  it('renders one rail per cell at its lane x, with start/end glyph', () => {
    render(<BgGutter row={{ cells: [
      { lane: 0, agentId: 'a', color: '#7da7ff', marker: 'start' },
      { lane: 1, agentId: 'b', color: '#41c285', marker: 'mid' },
    ], dense: 0 }} />);
    const rails = screen.getAllByTestId('gutter-rail');
    expect(rails).toHaveLength(2);
    expect(screen.getByTestId('gutter-start')).toBeInTheDocument();
  });
  it('dense → single neutral spine, no per-agent rails', () => {
    render(<BgGutter row={{ cells: [], dense: 5 }} />);
    expect(screen.queryAllByTestId('gutter-rail')).toHaveLength(0);
    expect(screen.getByTestId('gutter-dense')).toBeInTheDocument();
  });
  it('no row → empty gutter (keeps the column width)', () => {
    const { container } = render(<BgGutter row={undefined} />);
    expect(container.querySelector('[data-testid="gutter"]')).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd webui && npx vitest run src/components/replay/stream/__tests__/BgGutter.test.tsx`
Expected: FAIL (module not found).

- [ ] **Step 3: Implement `BgGutter.tsx`**

```tsx
// webui/src/components/replay/stream/BgGutter.tsx
// Fixed-width (~14px) left gutter painting the background-subagent hairlines for
// ONE stream row. Width is CONSTANT regardless of concurrency: ≤3 agents pack as
// rails at a 4px pitch; ≥4 collapse to a single neutral spine (the count rides
// the row's own bg-marker chip). Height follows the row (align-self stretch) so
// the virtualizer's measured row height is unchanged.
import type { GutterRow } from './streamModel';
import styles from './BgGutter.module.css';

const PITCH = 4; // px between lane rails
const X0 = 3; // px left inset of lane 0

export function BgGutter({ row }: { row: GutterRow | undefined }) {
  return (
    <div data-testid="gutter" className={styles.gutter} aria-hidden>
      {row && row.dense > 0 && <div data-testid="gutter-dense" className={styles.dense} />}
      {row && row.dense === 0 && row.cells.map((c) => {
        const left = X0 + c.lane * PITCH;
        return (
          <div key={c.agentId} className={styles.lane} style={{ left }}>
            <div data-testid="gutter-rail" className={styles.rail} style={{ background: c.color }} />
            {c.marker === 'start' && (
              <div data-testid="gutter-start" className={styles.start} style={{ boxShadow: `0 0 0 1.6px ${c.color}` }} />
            )}
            {c.marker === 'end' && (
              <div data-testid="gutter-end" className={styles.end} style={{ background: c.color }} />
            )}
          </div>
        );
      })}
    </div>
  );
}
```

`BgGutter.module.css`:

```css
.gutter { position: relative; flex: none; width: 14px; align-self: stretch; }
.lane { position: absolute; top: 0; bottom: 0; }
.rail { position: absolute; top: 0; bottom: 0; width: 2px; border-radius: 1px; }
.start { position: absolute; top: 7px; left: -2px; width: 7px; height: 7px; border-radius: 2px; background: var(--wimcc-bg, #0b0d12); }
.end { position: absolute; bottom: 7px; left: -2px; width: 7px; height: 7px; border-radius: 50%; box-shadow: 0 0 0 1.6px var(--wimcc-bg, #0b0d12); }
.dense { position: absolute; top: 0; bottom: 0; left: 6px; width: 2px; border-radius: 1px; background: var(--wimcc-fg-subtle, #6a7180); opacity: 0.7; }
```

- [ ] **Step 4: Run to verify it passes**

Run: `cd webui && npx vitest run src/components/replay/stream/__tests__/BgGutter.test.tsx`
Expected: PASS.

- [ ] **Step 5: Integrate into `ConversationStream.tsx`**

Add imports + memo + wrap rows. Near the other imports:
```tsx
import { computeBgGutter } from './streamModel';
import { BgGutter } from './BgGutter';
```
After `const auto = useAutoscroll(...)` (or near the top of the component body):
```tsx
  const gutterByRow = useMemo(() => computeBgGutter(items), [items]);
```
Wrap the VIRTUAL row body (replace `{renderItem(item)}` inside the absolute row div):
```tsx
                >
                  <div className={styles.row}>
                    <BgGutter row={gutterByRow.get(item.id)} />
                    <div className={styles.rowBody}>{renderItem(item)}</div>
                  </div>
                </div>
```
And the FALLBACK row body the same way:
```tsx
            <div key={item.id} {...(rowEventId(item) ? { 'data-event-id': rowEventId(item) } : {})}>
              <div className={styles.row}>
                <BgGutter row={gutterByRow.get(item.id)} />
                <div className={styles.rowBody}>{renderItem(item)}</div>
              </div>
            </div>
```

`ConversationStream.module.css` — add:
```css
.row { display: flex; align-items: stretch; }
.rowBody { flex: 1 1 auto; min-width: 0; }
```

- [ ] **Step 6: Verify the whole suite + types**

Run: `cd webui && npx vitest run && npx tsc --noEmit`
Expected: PASS, 0 type errors. (If an existing ConversationStream test asserts exact row DOM, adjust it to the new wrapper.)

- [ ] **Step 7: Commit**

```bash
git add webui/src/components/replay/stream/BgGutter.tsx webui/src/components/replay/stream/BgGutter.module.css webui/src/components/replay/stream/__tests__/BgGutter.test.tsx webui/src/components/replay/stream/ConversationStream.tsx webui/src/components/replay/stream/ConversationStream.module.css
git commit -m "feat(webui): BgGutter — per-row 헤어라인 거터(14px 고정, virtualizer 높이 불변) ConversationStream 통합"
```

---

### Task 6: Browser smoke + implementation-notes

- [ ] **Step 1: Find a STATIC session with standalone background subagents (agent_id present).** Use the read-only API (active session live-mutates — smoke on an inactive one):
```bash
curl -s 'http://127.0.0.1:7878/v1/sessions' | python3 -m json.tool | head -40
```
Pick a non-active session id; confirm it has top-level sidechain-groups with agent_id by spot-checking `/v1/sessions/:id/events`.

- [ ] **Step 2: Smoke in browser** (Vite :5173 hot-reloads src). Navigate to `/sessions/<staticId>`, verify: gutter ~14px, continuous per-agent rail across interleaved main rows, ▢ start at the block, ✓ end at the last covered row, color matches the block header swatch, dense fallback if any region has ≥4. Screenshot for the user.

- [ ] **Step 3: Update `docs/implementation-notes.html`** — add a section for the hairline gutter: design (honest time axis + per-row painted lanes), the lane-packing/dense rule, the color single-source, and the data caveat (span-derived end, no `subagent_completed`).

- [ ] **Step 4: Commit docs**
```bash
git add docs/implementation-notes.html webui/public/wf-bg-hairline-proto.html
git commit -m "docs: 백그라운드 서브에이전트 헤어라인 거터 설계·데이터 caveat 기록 + 승인 프로토타입"
```

---

### Task 7: PR

- [ ] Verify branch + full gates one more time: `git branch --show-current` (= `feat/workflow-grouping`), `cd webui && npm run build && npx vitest run`, then from repo root `cargo fmt --check && cargo clippy -- -D warnings && cargo test` (CI parity).
- [ ] Push and open PR to `main` (rebase-linear repo; PR body summarizes the bg-subagent visibility epic incl. this gutter slice). No AI footer (project hook blocks it).

## Self-Review notes
- No backend/migration — `agent_id` + timestamps already available; do not claim `subagent_completed`/`duration_ms`.
- Gutter width is constant under concurrency (the user's hard constraint): ≤3 packed, ≥4 dense.
- Virtualizer safety: gutter is a height-neutral flex sibling; never changes row height → prepend anchor / geometric resize rule / scrollPendingRef untouched.
- Terminology "백그라운드" (not "동시") everywhere — see memory ui-term-background-not-concurrent.
