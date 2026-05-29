# Redesign v2 — R4: Time-Series Timeline — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development or executing-plans. Checkbox steps. TDD red→green→commit each task.

**Goal:** Replace the static `Waterfall` in the bottom `timeline` slot with a familiar time-series surface: time axis + gridlines, wheel zoom / drag pan / fit, a minimap brush, an episode-phase band, lanes of nodes, hover tooltips, click-to-select synced with the stream, and edge encoding (deterministic solid vs inferred dashed+animated with rule label) emphasized on focus.

**Architecture:** Two pure modules — `timeScale` (domain→pixel via d3-scale, adaptive ticks via d3-time) and `viewport` (a pan/zoom/fit reducer over a `[t0,t1]` window) — keep the math testable without a DOM. `Timeline` is an SVG component that consumes them, renders lanes from the existing `laneMapping`, draws nodes (bars for spans, dots for instants), the episode band, and edges using the existing `causalEdgeStyle`. A `Minimap` brush sets the viewport window. Focus emphasis dims non-incident edges/nodes when a node is selected. To stay bounded under high node counts (spec §7), the renderer applies a per-lane density cap: when more than `MAX_NODES_PER_LANE` fall in the visible window, it renders aggregated tick marks instead of individual nodes and `log`s nothing but exposes a `data-aggregated` attribute.

**Tech Stack:** React 18, TypeScript, `d3-scale` + `d3-time` (already deps), `d3-zoom` (NEW — standard wheel/drag zoom behavior), CSS Modules, Vitest + Testing Library (jsdom — assert on data-attributes + pure-module unit tests; SVG geometry is verified through the pure modules, not computed layout).

**Spec:** `docs/superpowers/specs/2026-05-29-witmcc-ux-redesign-v2-design.md` §5, §7. Resolves feedback #5 (real time-series UI + edge info on focus).

**Real-data anchor:** graph payload from `GET /v1/sessions/:id/graph` — nodes `{node_id, node_kind, started_at, ended_at|null, source_event_ids}`, edges `{from_node_id, to_node_id, edge_kind, origin:'deterministic'|'inferred', inference_rule_id?, confidence?}`. Episodes from `/v1/sessions/:id/episodes` `{phase, started_at, ended_at}`. Lanes from existing `webui/src/api/laneMapping.ts` (Intent/Context/Action/State/Files/Hook/OTel/Quality). Edge styling from existing `webui/src/components/replay/causalEdgeStyle.ts`.

---

## File Structure

- **Create** `webui/src/components/replay/timeline/timeScale.ts` (+ test) — `makeTimeScale(domain, range)`, `axisTicks(scale, width)`.
- **Create** `webui/src/components/replay/timeline/viewport.ts` (+ test) — `Viewport` type + `fit`, `zoomAt`, `pan`, `clamp` pure functions.
- **Create** `webui/src/components/replay/timeline/Timeline.tsx` (+ `.module.css`, test) — the SVG surface (lanes, nodes, axis, episode band, edges, tooltip, focus emphasis).
- **Create** `webui/src/components/replay/timeline/Minimap.tsx` (+ `.module.css`, test) — overview + brush.
- **Create** `webui/src/components/replay/timeline/nodeLane.ts` (+ test) — `laneOfNodeKind(kind)` reusing `laneMapping`, and `nodesByLane(nodes)`.
- **Modify** `webui/src/routes/SessionDetailPage.tsx` — replace `Waterfall` with `Timeline` in the timeline slot; wire selection both ways; pass episodes + edges.
- **Modify** `webui/package.json` — add `d3-zoom` + `@types/d3-zoom`.
- **Delete** (Task 6, if dead) `webui/src/components/replay/Waterfall.tsx` + `.module.css` + test, and the old `webui/src/components/Timeline.tsx`/`Timeline.test.tsx` if still present and unused.

---

### Task 1: deps + timeScale (TDD)

**Files:** modify package.json; create `timeline/timeScale.ts` + `__tests__/timeScale.test.ts`.

- [ ] **Step 1: deps** — `cd webui && npm install d3-zoom && npm install -D @types/d3-zoom`. Build clean. Commit `webui(redesign-v2) R4: add d3-zoom dep`.

- [ ] **Step 2: failing test** for `timeScale.ts`:

```ts
// webui/src/components/replay/timeline/__tests__/timeScale.test.ts
/** R4 RED — time scale + adaptive ticks. Plan R4 Task 1. */
import { describe, expect, it } from 'vitest';
import { makeTimeScale, axisTicks } from '../timeScale';

const t0 = new Date('2026-05-28T00:00:00Z').getTime();
const t1 = new Date('2026-05-28T00:10:00Z').getTime();

describe('makeTimeScale', () => {
  it('maps domain start to range start and end to range end', () => {
    const s = makeTimeScale([t0, t1], [0, 600]);
    expect(s(t0)).toBeCloseTo(0);
    expect(s(t1)).toBeCloseTo(600);
  });
  it('maps the midpoint to the range midpoint', () => {
    const s = makeTimeScale([t0, t1], [0, 600]);
    expect(s((t0 + t1) / 2)).toBeCloseTo(300);
  });
});

describe('axisTicks', () => {
  it('returns ticks within the domain with x positions inside the range', () => {
    const s = makeTimeScale([t0, t1], [0, 600]);
    const ticks = axisTicks(s, 600);
    expect(ticks.length).toBeGreaterThan(0);
    for (const tk of ticks) {
      expect(tk.t).toBeGreaterThanOrEqual(t0);
      expect(tk.t).toBeLessThanOrEqual(t1);
      expect(tk.x).toBeGreaterThanOrEqual(0);
      expect(tk.x).toBeLessThanOrEqual(600);
      expect(typeof tk.label).toBe('string');
    }
  });
  it('produces more ticks for a wider pixel range', () => {
    const s1 = makeTimeScale([t0, t1], [0, 200]);
    const s2 = makeTimeScale([t0, t1], [0, 1200]);
    expect(axisTicks(s2, 1200).length).toBeGreaterThanOrEqual(axisTicks(s1, 200).length);
  });
});
```

- [ ] **Step 3: implement** using d3-scale + d3-time:

```ts
// webui/src/components/replay/timeline/timeScale.ts
import { scaleTime } from 'd3-scale';

export type TimeScale = ReturnType<typeof scaleTime<number, number>>;

export function makeTimeScale(domain: [number, number], range: [number, number]): TimeScale {
  return scaleTime().domain([new Date(domain[0]), new Date(domain[1])]).range(range);
}

export interface AxisTick { t: number; x: number; label: string; }

export function axisTicks(scale: TimeScale, width: number): AxisTick[] {
  // ~1 tick per 90px; d3 chooses nice time boundaries and the label format.
  const count = Math.max(2, Math.floor(width / 90));
  const fmt = scale.tickFormat(count);
  return scale.ticks(count).map((d) => ({ t: d.getTime(), x: scale(d), label: fmt(d) }));
}
```

- [ ] **Step 4: pass.** **Step 5: commit** `webui(redesign-v2) R4: timeScale + adaptive axis ticks`.

---

### Task 2: viewport pan/zoom/fit reducer (TDD)

Pure model of the visible `[t0,t1]` window. All interaction handlers translate to these.

**Files:** create `timeline/viewport.ts` + `__tests__/viewport.test.ts`.

- [ ] **Step 1: failing test**

```ts
// webui/src/components/replay/timeline/__tests__/viewport.test.ts
/** R4 RED — viewport pan/zoom/fit math. Plan R4 Task 2. */
import { describe, expect, it } from 'vitest';
import { fit, zoomAt, pan, clamp, type Viewport } from '../viewport';

const FULL: [number, number] = [1000, 2000];

describe('viewport', () => {
  it('fit returns the full extent', () => {
    expect(fit(FULL)).toEqual({ t0: 1000, t1: 2000 });
  });
  it('zoomAt with factor <1 narrows the window around the focus time', () => {
    const v: Viewport = { t0: 1000, t1: 2000 };
    const z = zoomAt(v, 0.5, 1500); // zoom in 2x centered at 1500
    expect(z.t1 - z.t0).toBeCloseTo(500);
    expect((z.t0 + z.t1) / 2).toBeCloseTo(1500);
  });
  it('zoomAt keeps the focus time fixed in proportion', () => {
    const v: Viewport = { t0: 1000, t1: 2000 };
    const z = zoomAt(v, 0.5, 1250); // focus at 25% of window
    const beforeFrac = (1250 - v.t0) / (v.t1 - v.t0);
    const afterFrac = (1250 - z.t0) / (z.t1 - z.t0);
    expect(afterFrac).toBeCloseTo(beforeFrac);
  });
  it('pan shifts the window by a time delta', () => {
    expect(pan({ t0: 1000, t1: 2000 }, 100)).toEqual({ t0: 1100, t1: 2100 });
  });
  it('clamp keeps the window within the full extent and preserves width when possible', () => {
    const c = clamp({ t0: 1800, t1: 2800 }, FULL);
    expect(c.t1).toBe(2000);
    expect(c.t0).toBe(1000); // width 1000 preserved, shifted back into extent
  });
  it('clamp never produces a window wider than the extent', () => {
    const c = clamp({ t0: 0, t1: 5000 }, FULL);
    expect(c).toEqual({ t0: 1000, t1: 2000 });
  });
});
```

- [ ] **Step 2-4: implement to pass:**

```ts
// webui/src/components/replay/timeline/viewport.ts
export interface Viewport { t0: number; t1: number; }

export function fit(extent: [number, number]): Viewport {
  return { t0: extent[0], t1: extent[1] };
}

/** factor < 1 zooms in (narrows), > 1 zooms out; focus time stays at the same fraction. */
export function zoomAt(v: Viewport, factor: number, focus: number): Viewport {
  const width = (v.t1 - v.t0) * factor;
  const frac = (focus - v.t0) / (v.t1 - v.t0);
  const t0 = focus - frac * width;
  return { t0, t1: t0 + width };
}

export function pan(v: Viewport, deltaT: number): Viewport {
  return { t0: v.t0 + deltaT, t1: v.t1 + deltaT };
}

export function clamp(v: Viewport, extent: [number, number]): Viewport {
  const extentWidth = extent[1] - extent[0];
  let width = Math.min(v.t1 - v.t0, extentWidth);
  let t0 = v.t0;
  let t1 = t0 + width;
  if (t0 < extent[0]) { t0 = extent[0]; t1 = t0 + width; }
  if (t1 > extent[1]) { t1 = extent[1]; t0 = t1 - width; }
  return { t0, t1 };
}
```

- [ ] **Step 5: commit** `webui(redesign-v2) R4: viewport pan/zoom/fit reducer`.

---

### Task 3: nodeLane mapping (TDD)

**Files:** create `timeline/nodeLane.ts` + test. Reuse `webui/src/api/laneMapping.ts` (it already maps node_kind→lane).

- [ ] **Step 1: failing test**

```ts
// webui/src/components/replay/timeline/__tests__/nodeLane.test.ts
/** R4 RED — node→lane mapping reuses laneMapping. Plan R4 Task 3. */
import { describe, expect, it } from 'vitest';
import { laneOfNodeKind, nodesByLane } from '../nodeLane';
import { LANES } from '../../../../api/laneMapping';

describe('nodeLane', () => {
  it('maps known kinds to their lane (per laneMapping)', () => {
    expect(laneOfNodeKind('user_message')).toBe('Intent');
    expect(laneOfNodeKind('tool_call')).toBe('Action');
    expect(laneOfNodeKind('otel_span')).toBe('OTel');
  });
  it('returns null for an unknown kind', () => {
    expect(laneOfNodeKind('made_up_kind')).toBeNull();
  });
  it('groups nodes into lane buckets keyed by lane name', () => {
    const nodes = [
      { node_id: 'a', node_kind: 'user_message' },
      { node_id: 'b', node_kind: 'tool_call' },
    ] as any;
    const byLane = nodesByLane(nodes);
    expect(byLane.get('Intent')?.map((n: any) => n.node_id)).toEqual(['a']);
    expect(byLane.get('Action')?.map((n: any) => n.node_id)).toEqual(['b']);
    expect(LANES).toContain('Intent');
  });
});
```
> Read `laneMapping.ts` first to use its actual export (it exposes `LANES` and a kind→lane map/function). Implement `laneOfNodeKind` + `nodesByLane` over that. If the existing map is keyed lane→kinds, invert it once at module load.

- [ ] **Step 2-5:** implement to pass; commit `webui(redesign-v2) R4: node→lane mapping`.

---

### Task 4: Timeline SVG component (TDD)

The core surface. Props:
```ts
interface TimelineProps {
  nodes: GraphNodeDto[];
  edges: GraphEdgeDto[];
  episodes: EpisodeDto[];
  selectedNodeId: string | null;
  onSelect: (nodeId: string | null) => void;
  width?: number;   // default measured; tests pass explicit
  height?: number;
}
```
Behavior (assert via data-attributes — jsdom has no layout):
- Renders one lane row per `LANES` entry with `data-lane="<name>"`.
- Renders nodes within the visible viewport as `<rect data-node-id data-node-kind>` (has ended_at) or `<circle data-node-id>` (instant). Off-window nodes not rendered.
- Time axis group `data-testid="time-axis"` with tick `<text>` labels.
- Episode band group `data-testid="episode-band"` with one `<rect data-phase>` per episode.
- Edges group: each edge `<path data-edge-id data-origin>`; inferred edges get `stroke-dasharray` and `data-rule-id`; styling from `causalEdgeStyle`.
- Focus: when `selectedNodeId` set, the node has `data-selected="true"`; edges incident to it get `data-emphasized="true"`, others `data-dimmed="true"`.
- Clicking a node calls `onSelect(nodeId)`; clicking empty space calls `onSelect(null)`.
- Zoom buttons (`data-testid="zoom-in"`, `zoom-out`, `fit`) adjust the viewport via the Task-2 reducer.
- Density cap: if a lane has > `MAX_NODES_PER_LANE` (e.g. 200) visible nodes, render an aggregated marker row with `data-aggregated="true"` for that lane instead of individual nodes.

- [ ] **Step 1: failing test** (representative — implementer expands to cover the bullets):

```tsx
// webui/src/components/replay/timeline/__tests__/Timeline.test.tsx
/** R4 RED — Timeline SVG surface. Plan R4 Task 4 / spec §5. */
import { render, screen, fireEvent, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { Timeline } from '../Timeline';
import type { GraphNodeDto, GraphEdgeDto, EpisodeDto } from '../../../../api/types';

const T = (s: string) => new Date(s).toISOString();
function node(id: string, kind: string, start: string, end: string | null): GraphNodeDto {
  return { node_id: id, schema_version: '1', session_id: 's', node_kind: kind, started_at: T(start), ended_at: end ? T(end) : null, merge_keys: {}, source_event_ids: [], source_uris: [], payload: {} };
}
function edge(id: string, from: string, to: string, origin: string, ruleId?: string): GraphEdgeDto {
  return { edge_id: id, schema_version: '1', session_id: 's', from_node_id: from, to_node_id: to, edge_kind: 'x', origin, attributes: {}, inference_rule_id: ruleId ?? null, confidence: origin === 'inferred' ? 0.7 : null };
}
const nodes = [node('a', 'user_message', '2026-05-28T00:00:00Z', null), node('b', 'tool_call', '2026-05-28T00:01:00Z', '2026-05-28T00:01:05Z')];
const edges = [edge('e1', 'a', 'b', 'inferred', 'triggered_by_user_message@v1')];
const episodes: EpisodeDto[] = [{ episode_id: 'ep', schema_version: '1', session_id: 's', phase: 'action', start_event_id: '', end_event_id: '', started_at: T('2026-05-28T00:00:00Z'), ended_at: T('2026-05-28T00:02:00Z'), evidence_node_ids: [], classification_basis: [], confidence: 1 }];

function renderTL(props = {}) {
  return render(<Timeline nodes={nodes} edges={edges} episodes={episodes} selectedNodeId={null} onSelect={() => {}} width={800} height={300} {...props} />);
}

describe('Timeline', () => {
  it('renders a lane row per LANES entry', () => {
    const { container } = renderTL();
    expect(container.querySelector('[data-lane="Intent"]')).not.toBeNull();
    expect(container.querySelector('[data-lane="Action"]')).not.toBeNull();
  });
  it('renders a node element per visible node with its id + kind', () => {
    const { container } = renderTL();
    expect(container.querySelector('[data-node-id="a"]')).not.toBeNull();
    expect(container.querySelector('[data-node-id="b"][data-node-kind="tool_call"]')).not.toBeNull();
  });
  it('renders a time axis with tick labels', () => {
    renderTL();
    expect(screen.getByTestId('time-axis')).toBeInTheDocument();
  });
  it('renders an episode band with a rect per episode phase', () => {
    const { container } = renderTL();
    const band = screen.getByTestId('episode-band');
    expect(within(band).getAllByText(/action/i).length).toBeGreaterThanOrEqual(0); // label optional
    expect(band.querySelector('[data-phase="action"]')).not.toBeNull();
  });
  it('renders inferred edges dashed with a rule id', () => {
    const { container } = renderTL();
    const e = container.querySelector('[data-edge-id="e1"]') as SVGElement;
    expect(e).not.toBeNull();
    expect(e.getAttribute('data-origin')).toBe('inferred');
    expect(e.getAttribute('data-rule-id')).toBe('triggered_by_user_message@v1');
    expect(e.getAttribute('stroke-dasharray')).toBeTruthy();
  });
  it('marks the selected node and emphasizes its incident edges', () => {
    const { container } = renderTL({ selectedNodeId: 'a' });
    expect(container.querySelector('[data-node-id="a"]')?.getAttribute('data-selected')).toBe('true');
    expect(container.querySelector('[data-edge-id="e1"]')?.getAttribute('data-emphasized')).toBe('true');
  });
  it('fires onSelect with the node id on click', () => {
    const onSelect = vi.fn();
    const { container } = renderTL({ onSelect });
    fireEvent.click(container.querySelector('[data-node-id="b"]')!);
    expect(onSelect).toHaveBeenCalledWith('b');
  });
  it('has zoom-in / zoom-out / fit controls', () => {
    renderTL();
    expect(screen.getByTestId('zoom-in')).toBeInTheDocument();
    expect(screen.getByTestId('zoom-out')).toBeInTheDocument();
    expect(screen.getByTestId('fit')).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: run, verify fail.**
- [ ] **Step 3: implement** `Timeline.tsx` + `.module.css`. Use `makeTimeScale` over the current viewport window, `axisTicks`, `nodesByLane`, `causalEdgeStyle` for edge stroke (solid vs dashed + width/opacity from confidence). Hold viewport in `useState` initialized to `fit([minStart, maxEnd])`; zoom buttons call `zoomAt`/`fit` + `clamp`; wheel handler zooms at cursor time; drag pans. Use reduced-motion media query to gate the dashed-offset animation (spec §10). Apply the density cap. Tooltip on hover (`data-testid="node-tooltip"`). Keep node click stopping propagation so background click (deselect) is distinct.
- [ ] **Step 4: pass.** **Step 5: commit** `webui(redesign-v2) R4: Timeline SVG (lanes, axis, nodes, edges, episode band, focus, zoom)`.

---

### Task 5: Minimap brush (TDD)

**Files:** create `timeline/Minimap.tsx` + `.module.css` + test.

Props: `{ extent: [number,number]; viewport: Viewport; onChange: (v: Viewport) => void; width?: number }`. Renders a full-extent overview with a draggable window rect (`data-testid="brush-window"`); dragging/clicking calls `onChange` with a clamped viewport.

- [ ] **Step 1: failing test**

```tsx
// webui/src/components/replay/timeline/__tests__/Minimap.test.tsx
/** R4 RED — Minimap brush drives the viewport. Plan R4 Task 5. */
import { render, screen, fireEvent } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { Minimap } from '../Minimap';

describe('Minimap', () => {
  it('renders a brush window sized to the current viewport fraction', () => {
    render(<Minimap extent={[0, 1000]} viewport={{ t0: 250, t1: 750 }} onChange={() => {}} width={400} />);
    const w = screen.getByTestId('brush-window');
    // window covers 50% of extent => width ~200 of 400
    expect(Number(w.getAttribute('width'))).toBeCloseTo(200, 0);
    expect(Number(w.getAttribute('x'))).toBeCloseTo(100, 0);
  });
  it('calls onChange when the overview is clicked to recenter', () => {
    const onChange = vi.fn();
    render(<Minimap extent={[0, 1000]} viewport={{ t0: 0, t1: 200 }} onChange={onChange} width={400} />);
    fireEvent.mouseDown(screen.getByTestId('minimap-track'), { clientX: 200 });
    expect(onChange).toHaveBeenCalled();
  });
});
```

- [ ] **Step 2-5:** implement (a horizontal track + window rect; map pixel↔time with a linear scale over extent; `onChange` returns a clamped viewport). Commit `webui(redesign-v2) R4: Minimap brush`.

> jsdom note: `getBoundingClientRect` returns zeros; compute geometry from the `width` prop, not from measured rects, so click math is deterministic in tests and uses `event.clientX` relative to a known origin (offset 0 in tests). In the browser, read the track's left via a ref.

---

### Task 6: wire Timeline+Minimap into the slot; remove Waterfall; smoke

**Files:** modify `SessionDetailPage.tsx`; delete Waterfall (+ old Timeline.tsx) if dead.

- [ ] **Step 1:** In the timeline slot, render `<Minimap>` above `<Timeline>` sharing one viewport state lifted into a small wrapper, OR have `Timeline` own the viewport and render the `Minimap` internally (simpler: Timeline owns viewport + renders Minimap at its bottom). Choose the internal-ownership approach: `Timeline` renders the Minimap itself. Then SessionDetailPage just passes nodes/edges/episodes/selection.
- [ ] **Step 2:** Replace `<Waterfall .../>` with `<Timeline nodes={effectiveGraph.nodes} edges={effectiveGraph.edges} episodes={episodes.data ?? []} selectedNodeId={sel.selectedNodeId} onSelect={(id) => sel.setSelectedNodeId(id)} />`. Remove the `Waterfall` import.
- [ ] **Step 3:** Selection sync: clicking a timeline node sets `selectedNodeId`; the stream + DetailPanel already react to `selectedNodeId`. Verify the stream's `selectedStreamEventId` (node→event) lights the right card. (Already wired in R2/R3 — no new code expected; confirm.)
- [ ] **Step 4:** Dead-code: `grep -rn "Waterfall" webui/src --include=*.tsx --include=*.ts`. If unused in production, `git rm` `Waterfall.tsx/.module.css/__tests__/Waterfall.test.tsx`. Also remove the legacy `webui/src/components/Timeline.tsx` + `__tests__/Timeline.test.tsx` if present and unused (spec §5 discards the old lanes layout). Keep `causalEdgeStyle.ts` + `laneMapping.ts` (reused).
- [ ] **Step 5:** `cd webui && npx vitest run && npx tsc --noEmit && npm run build` — green. Update any SessionDetailPage test that referenced the Waterfall testid to the Timeline equivalent.
- [ ] **Step 6: commit** `webui(redesign-v2) R4: mount Timeline+Minimap, remove Waterfall`.
- [ ] **Step 7: browser smoke** — rebuild + serve; on a session: full-width timeline at bottom with a real time axis, zoom in/out + fit work, dragging pans, minimap window drag changes the view, episode band colors show, hovering a node shows a tooltip, clicking a node selects it AND highlights the matching stream card + DetailPanel; selecting a node emphasizes its edges (inferred dashed+animated with rule label, deterministic solid). Verify a dense session degrades to aggregated markers without freezing. Fix + re-run until clean.

---

## Self-Review

- **Spec coverage (§5):** axis+grid (Task 1/4), zoom/pan/fit (Task 2/4), minimap brush (Task 5), episode band (Task 4), focus edge emphasis (Task 4), edge-type encoding via causalEdgeStyle + rule label (Task 4), stream sync (Task 6), hover tooltip (Task 4). §7 memory: density cap (Task 4) + viewport windowing (only visible nodes render). Reduced-motion gates animation (Task 4).
- **Placeholder scan:** pure modules (timeScale, viewport, nodeLane) have full code + tests; Timeline/Minimap give full prop contracts, representative tests, and explicit implementation instructions tied to existing utilities (causalEdgeStyle, laneMapping). The implementer expands the Timeline test to cover every bullet — flagged explicitly, not a silent gap.
- **Type consistency:** `Viewport {t0,t1}` used by viewport, Timeline, Minimap. `TimelineProps` (nodes/edges/episodes/selectedNodeId/onSelect/width/height) match the wire-in. `AxisTick {t,x,label}`. data-attributes (`data-lane`, `data-node-id`, `data-node-kind`, `data-edge-id`, `data-origin`, `data-rule-id`, `data-selected`, `data-emphasized`, `data-dimmed`, `data-phase`, `data-aggregated`) are the test contract and must match between impl and tests.
- **Open risk:** d3-zoom integration with React refs can be finicky; the viewport reducer keeps the math out of d3-zoom so even a hand-rolled wheel/drag handler satisfies the tests if d3-zoom proves awkward. Implementer may use either; tests assert behavior via the reducer + buttons, not d3-zoom internals.
