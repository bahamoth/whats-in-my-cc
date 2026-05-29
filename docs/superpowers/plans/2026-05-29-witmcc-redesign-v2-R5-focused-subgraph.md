# Redesign v2 — R5: Focused Insight Subgraph — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development or executing-plans. Checkbox steps, TDD red→green→commit.

**Goal:** When a node is selected, show its 1–2 hop causal neighborhood ("what triggered this, what it caused") as a small graph inside the Insight tab — replacing the removed whole-session graph dump with a findable, comprehensible focused view.

**Architecture:** A pure `neighborhood(graph, nodeId, hops)` computes the bounded subgraph (selected node + nodes reachable within `hops` edge-steps in either direction, plus the connecting edges). `FocusedInsightGraph` lays it out left→right with dagre and renders it with `@xyflow/react` (both already deps), reusing `causalEdgeStyle` for edge encoding consistent with the Timeline. It mounts inside `InsightTab`, above the findings list. Hop count defaults to 1 with an expand control to 2 (the user will re-judge against the rendered result — spec §6, §11). Bounded by hop count → never renders the whole graph (spec §7).

**Tech Stack:** React 18, TypeScript, `@xyflow/react` + `dagre` (already deps), `causalEdgeStyle`, CSS Modules, Vitest + Testing Library. Note: `@xyflow/react` needs DOM measurement that jsdom lacks — test the **pure neighborhood module** thoroughly, and test `FocusedInsightGraph` at the contract level (renders nodes/edges it was given, exposes data-attributes), mocking ResizeObserver as the existing tests do if needed.

**Spec:** §6 (focused subgraph replaces whole-session graph) + §7 (bounded) + §11 (hop count is a re-judgeable default). Resolves feedback #6.

**Real-data anchor:** graph payload nodes/edges as in R4. Edges are directed `from_node_id → to_node_id`. The selected node id comes from `ReplaySelection.selectedNodeId`.

---

## File Structure

- **Create** `webui/src/components/replay/insight/neighborhood.ts` (+ test) — pure subgraph extraction.
- **Create** `webui/src/components/replay/insight/FocusedInsightGraph.tsx` (+ `.module.css`, test) — dagre + React Flow render of the neighborhood.
- **Modify** `webui/src/components/replay/detail/InsightTab.tsx` (+ test) — accept `graph`, `selectedNodeId`, `onSelectNode`; render `FocusedInsightGraph` above the findings list.
- **Modify** `webui/src/components/replay/detail/DetailPanel.tsx` — thread `graph`, `selectedNodeId`, `onSelectNode` to InsightTab.
- **Modify** `webui/src/routes/SessionDetailPage.tsx` — pass `effectiveGraph`, `sel.selectedNodeId`, and a node-select callback to DetailPanel.

---

### Task 1: neighborhood extraction (TDD)

**Files:** create `insight/neighborhood.ts` + `__tests__/neighborhood.test.ts`.

Semantics: `neighborhood(nodes, edges, centerId, hops)` returns `{ nodes: GraphNodeDto[], edges: GraphEdgeDto[] }` where nodes are those within `hops` undirected edge-steps of `centerId` (so both upstream causes and downstream effects appear), and edges are those whose both endpoints are in the kept set. Returns empty when centerId is null/absent. Deterministic order (BFS order, center first).

- [ ] **Step 1: failing test**

```ts
// webui/src/components/replay/insight/__tests__/neighborhood.test.ts
/** R5 RED — neighborhood extracts a bounded subgraph around the center. Plan R5 Task 1. */
import { describe, expect, it } from 'vitest';
import { neighborhood } from '../neighborhood';
import type { GraphNodeDto, GraphEdgeDto } from '../../../../api/types';

function n(id: string): GraphNodeDto {
  return { node_id: id, schema_version: '1', session_id: 's', node_kind: 'k', started_at: '', ended_at: null, merge_keys: {}, source_event_ids: [], source_uris: [], payload: {} };
}
function e(id: string, from: string, to: string): GraphEdgeDto {
  return { edge_id: id, schema_version: '1', session_id: 's', from_node_id: from, to_node_id: to, edge_kind: 'x', origin: 'deterministic', attributes: {}, inference_rule_id: null, confidence: null };
}
// chain a -> b -> c -> d ; plus b -> x
const nodes = ['a', 'b', 'c', 'd', 'x'].map(n);
const edges = [e('e1', 'a', 'b'), e('e2', 'b', 'c'), e('e3', 'c', 'd'), e('e4', 'b', 'x')];

describe('neighborhood', () => {
  it('returns empty for a null/absent center', () => {
    expect(neighborhood(nodes, edges, null, 1)).toEqual({ nodes: [], edges: [] });
    expect(neighborhood(nodes, edges, 'zzz', 1)).toEqual({ nodes: [], edges: [] });
  });

  it('1 hop around b includes a, b, c, x (upstream + downstream) but not d', () => {
    const sub = neighborhood(nodes, edges, 'b', 1);
    expect(new Set(sub.nodes.map((nn) => nn.node_id))).toEqual(new Set(['a', 'b', 'c', 'x']));
    expect(sub.nodes.find((nn) => nn.node_id === 'd')).toBeUndefined();
  });

  it('keeps only edges whose both endpoints are in the kept set', () => {
    const sub = neighborhood(nodes, edges, 'b', 1);
    const ids = new Set(sub.edges.map((ed) => ed.edge_id));
    expect(ids).toEqual(new Set(['e1', 'e2', 'e4'])); // not e3 (c->d, d excluded)
  });

  it('2 hops around b reaches d', () => {
    const sub = neighborhood(nodes, edges, 'b', 2);
    expect(sub.nodes.find((nn) => nn.node_id === 'd')).toBeDefined();
    expect(sub.edges.map((ed) => ed.edge_id)).toContain('e3');
  });

  it('lists the center node first', () => {
    const sub = neighborhood(nodes, edges, 'b', 1);
    expect(sub.nodes[0].node_id).toBe('b');
  });
});
```

- [ ] **Step 2-4: implement**

```ts
// webui/src/components/replay/insight/neighborhood.ts
import type { GraphNodeDto, GraphEdgeDto } from '../../../api/types';

export interface SubGraph { nodes: GraphNodeDto[]; edges: GraphEdgeDto[]; }

export function neighborhood(
  nodes: GraphNodeDto[],
  edges: GraphEdgeDto[],
  centerId: string | null,
  hops: number,
): SubGraph {
  if (!centerId || !nodes.some((n) => n.node_id === centerId)) {
    return { nodes: [], edges: [] };
  }
  // undirected adjacency
  const adj = new Map<string, string[]>();
  for (const e of edges) {
    (adj.get(e.from_node_id) ?? adj.set(e.from_node_id, []).get(e.from_node_id)!).push(e.to_node_id);
    (adj.get(e.to_node_id) ?? adj.set(e.to_node_id, []).get(e.to_node_id)!).push(e.from_node_id);
  }
  // BFS to `hops`, center first
  const order: string[] = [centerId];
  const dist = new Map<string, number>([[centerId, 0]]);
  const queue = [centerId];
  while (queue.length) {
    const cur = queue.shift()!;
    const d = dist.get(cur)!;
    if (d >= hops) continue;
    for (const nb of adj.get(cur) ?? []) {
      if (!dist.has(nb)) {
        dist.set(nb, d + 1);
        order.push(nb);
        queue.push(nb);
      }
    }
  }
  const keep = new Set(order);
  const byId = new Map(nodes.map((n) => [n.node_id, n]));
  const subNodes = order.map((id) => byId.get(id)).filter((n): n is GraphNodeDto => !!n);
  const subEdges = edges.filter((e) => keep.has(e.from_node_id) && keep.has(e.to_node_id));
  return { nodes: subNodes, edges: subEdges };
}
```

- [ ] **Step 5: commit** `webui(redesign-v2) R5: neighborhood subgraph extraction`.

---

### Task 2: FocusedInsightGraph component (TDD)

Lays out the subgraph LR with dagre and renders with @xyflow/react. Each node shows its kind + a short id; the center node is marked; clicking a node calls `onSelectNode`. Edges use `causalEdgeStyle` (solid/dashed + width/opacity), inferred edges labelled with rule id. Includes a hop control (1↔2).

Because @xyflow/react requires layout APIs jsdom lacks, the test mocks `ResizeObserver` (mirror any existing setup; the deleted CausalGraph test did this — check `webui/src/test/setup.ts` or vitest config for a global mock and reuse). Assert on the React Flow node/edge data passed in (the component computes `nodes`/`edges` arrays and renders a container with `data-testid="focused-graph"`, `data-node-count`, `data-edge-count`, `data-hops`).

- [ ] **Step 1: failing test**

```tsx
// webui/src/components/replay/insight/__tests__/FocusedInsightGraph.test.tsx
/** R5 RED — FocusedInsightGraph renders the bounded neighborhood. Plan R5 Task 2 / spec §6. */
import { render, screen, fireEvent } from '@testing-library/react';
import { describe, expect, it, vi, beforeAll } from 'vitest';
import { FocusedInsightGraph } from '../FocusedInsightGraph';
import type { GraphNodeDto, GraphEdgeDto } from '../../../../api/types';

beforeAll(() => {
  // @xyflow/react needs ResizeObserver
  (global as any).ResizeObserver = class { observe() {} unobserve() {} disconnect() {} };
});

function n(id: string, kind = 'tool_call'): GraphNodeDto {
  return { node_id: id, schema_version: '1', session_id: 's', node_kind: kind, started_at: '', ended_at: null, merge_keys: {}, source_event_ids: [], source_uris: [], payload: {} };
}
function e(id: string, from: string, to: string, origin = 'deterministic'): GraphEdgeDto {
  return { edge_id: id, schema_version: '1', session_id: 's', from_node_id: from, to_node_id: to, edge_kind: 'x', origin, attributes: {}, inference_rule_id: origin === 'inferred' ? 'caused_repair@v1' : null, confidence: origin === 'inferred' ? 0.6 : null };
}
const nodes = [n('a', 'user_message'), n('b'), n('c')];
const edges = [e('e1', 'a', 'b', 'inferred'), e('e2', 'b', 'c')];

describe('FocusedInsightGraph', () => {
  it('renders an empty hint when no node is selected', () => {
    render(<FocusedInsightGraph nodes={nodes} edges={edges} selectedNodeId={null} onSelectNode={() => {}} />);
    expect(screen.getByText(/select a node/i)).toBeInTheDocument();
  });

  it('renders the focused-graph container with bounded node/edge counts for the 1-hop neighborhood of b', () => {
    render(<FocusedInsightGraph nodes={nodes} edges={edges} selectedNodeId="b" onSelectNode={() => {}} />);
    const g = screen.getByTestId('focused-graph');
    // 1 hop around b => a, b, c (3 nodes), e1 + e2 (2 edges)
    expect(g.getAttribute('data-node-count')).toBe('3');
    expect(g.getAttribute('data-edge-count')).toBe('2');
    expect(g.getAttribute('data-hops')).toBe('1');
  });

  it('toggles hop count via the expand control', () => {
    render(<FocusedInsightGraph nodes={nodes} edges={edges} selectedNodeId="b" onSelectNode={() => {}} />);
    fireEvent.click(screen.getByTestId('hop-toggle'));
    expect(screen.getByTestId('focused-graph').getAttribute('data-hops')).toBe('2');
  });
});
```
> Verify a global ResizeObserver mock isn't already provided by the test setup; if it is, drop the local `beforeAll`. Read the vitest setup file first.

- [ ] **Step 2: run, verify fail.**
- [ ] **Step 3: implement** `FocusedInsightGraph.tsx` (+ css). Use `neighborhood(nodes, edges, selectedNodeId, hops)` (hops in useState, default 1). Build dagre layout (LR, nodesep ~28, ranksep ~56) over the subgraph; map to `@xyflow/react` `Node[]`/`Edge[]`; edge style from `causalEdgeStyle`; inferred edge `label = inference_rule_id`. Center node gets a distinct class/data-attr. The container div carries `data-testid="focused-graph"`, `data-node-count`, `data-edge-count`, `data-hops`. A `data-testid="hop-toggle"` button flips hops 1↔2. Node click → `onSelectNode(id)`. Empty hint when `selectedNodeId` null or neighborhood empty. (Reuse the dagre+ReactFlow pattern; keep the component focused and small.)
- [ ] **Step 4: pass.** **Step 5: commit** `webui(redesign-v2) R5: FocusedInsightGraph (dagre + React Flow neighborhood)`.

---

### Task 3: integrate into InsightTab + wire-in (TDD)

**Files:** modify `InsightTab.tsx` (+ test), `DetailPanel.tsx`, `SessionDetailPage.tsx`.

- [ ] **Step 1: extend InsightTab test** — InsightTab now takes `findings`, `nodes`, `edges`, `selectedNodeId`, `onSelectNode`. It renders `FocusedInsightGraph` (assert `focused-graph` testid present when a node is selected) ABOVE the findings list (existing finding assertions still pass). Keep the empty-findings hint for the findings section. Mock ResizeObserver as in Task 2.

```tsx
// add to webui/src/components/replay/detail/__tests__/InsightTab.test.tsx
it('renders the focused subgraph above the findings for the selected node', () => {
  // ResizeObserver mocked in beforeAll (add if not present)
  render(<InsightTab findings={[]} nodes={[/* a,b,c as in neighborhood */]} edges={[]} selectedNodeId="b" onSelectNode={() => {}} />);
  expect(screen.getByTestId('focused-graph')).toBeInTheDocument();
});
```
(Use minimal node fixtures; the key assertion is the subgraph mounts. Keep existing finding tests, passing the new props with empty arrays / null where needed — update their render calls to the new signature.)

- [ ] **Step 2: run, verify fail** (InsightTab signature change breaks old calls / new assertion fails).
- [ ] **Step 3: implement** — update `InsightTab` props to `{ findings, nodes, edges, selectedNodeId, onSelectNode }`; render `<FocusedInsightGraph .../>` then the findings `<ul>`. Update `DetailPanel` to accept `nodes`, `edges`, `onSelectNode` and pass them + its `node?.node_id` (the selected id) into InsightTab. Update `SessionDetailPage` to pass `nodes={effectiveGraph.nodes}`, `edges={effectiveGraph.edges}`, `onSelectNode={(id) => sel.setSelectedNodeId(id)}` to DetailPanel.
- [ ] **Step 4: full suite + tsc + build** green. Update DetailPanel tests for the new props (pass empty arrays). Add the ResizeObserver mock to the vitest setup file if multiple tests now need it (cleaner than per-file).
- [ ] **Step 5: commit** `webui(redesign-v2) R5: mount FocusedInsightGraph in InsightTab`.

---

### Task 4: browser smoke

- [ ] Rebuild + serve. Select a node (stream card or timeline node) that participates in causal edges. In the right panel's **Insight** tab, confirm a small focused graph shows the node + its 1-hop neighbors with edges (inferred dashed + rule label, deterministic solid), the center node marked.
- [ ] Click `hop-toggle` → neighborhood grows to 2 hops.
- [ ] Click a neighbor node in the subgraph → selection moves to it (stream/timeline/detail follow).
- [ ] Confirm a node with findings shows findings below the subgraph.
- [ ] Fix + re-run until clean. (The user will re-judge the subgraph against this rendered result per spec §11 — capture a screenshot.)

---

## Self-Review

- **Spec coverage:** §6 focused subgraph (neighborhood + FocusedInsightGraph) replacing whole-session graph; lives in Insight tab; 1-hop default with 2-hop expand (§11 re-judgeable). §7 bounded by hops (neighborhood never returns whole graph). Edge encoding reuses causalEdgeStyle (consistent with Timeline). Resolves #6.
- **Placeholder scan:** neighborhood has full code + thorough tests; FocusedInsightGraph + InsightTab give full prop contracts, representative tests, and explicit implementation steps tied to existing deps (dagre, @xyflow/react, causalEdgeStyle). ResizeObserver mock flagged (verify/centralize in setup).
- **Type consistency:** `SubGraph {nodes,edges}`; `neighborhood(nodes,edges,centerId,hops)`; `FocusedInsightGraph` props (`nodes,edges,selectedNodeId,onSelectNode`); `InsightTab` extended props (`findings,nodes,edges,selectedNodeId,onSelectNode`); DetailPanel threads them. data-attributes (`focused-graph`, `data-node-count`, `data-edge-count`, `data-hops`, `hop-toggle`) are the test contract.
- **Open risk:** @xyflow/react in jsdom — tests assert on the computed counts/attributes (deterministic from neighborhood), not on React Flow's internal layout, so they don't depend on real measurement. Browser smoke validates the actual rendering.
