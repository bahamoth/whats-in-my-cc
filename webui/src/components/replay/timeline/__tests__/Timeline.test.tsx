// webui/src/components/replay/timeline/__tests__/Timeline.test.tsx
/** R4 RED — Timeline SVG surface. Plan R4 Task 4 / spec §5. */
import { render, screen, fireEvent } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { Timeline } from '../Timeline';
import type { GraphNodeDto, GraphEdgeDto } from '../../../../api/types';

const T = (s: string) => new Date(s).toISOString();
function node(id: string, kind: string, start: string, end: string | null, payload: unknown = {}): GraphNodeDto {
  return { node_id: id, schema_version: '1', session_id: 's', node_kind: kind, started_at: T(start), ended_at: end ? T(end) : null, merge_keys: {}, source_event_ids: [], source_uris: [], payload };
}
function edge(id: string, from: string, to: string, origin: string, ruleId?: string): GraphEdgeDto {
  return { edge_id: id, schema_version: '1', session_id: 's', from_node_id: from, to_node_id: to, edge_kind: 'x', origin, attributes: {}, inference_rule_id: ruleId ?? null, confidence: origin === 'inferred' ? 0.7 : null };
}
const nodes = [
  node('a', 'user_message', '2026-05-28T00:00:00Z', null),
  node('b', 'tool_call', '2026-05-28T00:01:00Z', '2026-05-28T00:01:05Z'),
];
const edges = [edge('e1', 'a', 'b', 'inferred', 'triggered_by_user_message@v1')];
const deterministicEdge = edge('e2', 'a', 'b', 'deterministic');

function renderTL(props = {}) {
  return render(<Timeline nodes={nodes} edges={edges} selectedNodeId={null} onSelect={() => {}} width={800} height={300} {...props} />);
}

describe('Timeline', () => {
  // --- Lane rows (empty lanes hidden, #4) ---
  it('renders a lane row only for lanes that have nodes (empty lanes hidden)', () => {
    // nodes: user_message → Intent, tool_call → Action. The other 6 lanes are
    // empty and must NOT render a row (keeps the surface compact + rows tall).
    const { container } = renderTL();
    expect(container.querySelector('[data-lane="Intent"]')).not.toBeNull();
    expect(container.querySelector('[data-lane="Action"]')).not.toBeNull();
    expect(container.querySelector('[data-lane="Context"]')).toBeNull();
    expect(container.querySelector('[data-lane="State"]')).toBeNull();
    expect(container.querySelector('[data-lane="Files"]')).toBeNull();
    expect(container.querySelector('[data-lane="Hook"]')).toBeNull();
    expect(container.querySelector('[data-lane="OTel"]')).toBeNull();
    expect(container.querySelector('[data-lane="Quality"]')).toBeNull();
  });

  // --- Nodes ---
  it('renders a node element per visible node with its id + kind', () => {
    const { container } = renderTL();
    expect(container.querySelector('[data-node-id="a"]')).not.toBeNull();
    expect(container.querySelector('[data-node-id="b"][data-node-kind="tool_call"]')).not.toBeNull();
  });

  it('renders a circle (data-node-id, no data-node-kind required to be rect) for instant nodes (no ended_at)', () => {
    const { container } = renderTL();
    // node 'a' has no ended_at → should be a circle
    const el = container.querySelector('[data-node-id="a"]');
    expect(el).not.toBeNull();
    expect(el!.tagName.toLowerCase()).toBe('circle');
  });

  it('renders a rect for span nodes (has ended_at)', () => {
    const { container } = renderTL();
    const el = container.querySelector('[data-node-id="b"]');
    expect(el).not.toBeNull();
    expect(el!.tagName.toLowerCase()).toBe('rect');
  });

  it('does not render off-window nodes (filters by viewport after zoom-in)', () => {
    // Three instant nodes at the extent's start / center / end. On mount the
    // viewport fits the full extent [0, 120s], so all three are visible.
    // Zoom-in halves the viewport around its center → [30s, 90s], which
    // deterministically excludes the start (0s) and end (120s) nodes while
    // keeping the center (60s) node. This locks the visibleByLane filter
    // (Timeline.tsx) rather than asserting the opposite of the test's name.
    const left = node('left', 'tool_call', '2026-05-28T00:00:00Z', null);
    const mid = node('mid', 'tool_call', '2026-05-28T00:01:00Z', null);
    const right = node('right', 'tool_call', '2026-05-28T00:02:00Z', null);
    const { container } = render(
      <Timeline
        nodes={[left, mid, right]}
        edges={[]}
        selectedNodeId={null}
        onSelect={() => {}}
        width={800}
        height={300}
      />
    );
    // All visible at the fitted extent.
    expect(container.querySelector('[data-node-id="left"]')).not.toBeNull();
    expect(container.querySelector('[data-node-id="mid"]')).not.toBeNull();
    expect(container.querySelector('[data-node-id="right"]')).not.toBeNull();

    // Zoom in once → window narrows to the middle half, dropping the edges.
    fireEvent.click(screen.getByTestId('zoom-in'));
    expect(container.querySelector('[data-node-id="mid"]')).not.toBeNull();
    expect(container.querySelector('[data-node-id="left"]')).toBeNull();
    expect(container.querySelector('[data-node-id="right"]')).toBeNull();
  });

  // --- Time axis ---
  it('renders a time axis with tick labels', () => {
    renderTL();
    expect(screen.getByTestId('time-axis')).toBeInTheDocument();
  });

  it('time axis contains text tick labels', () => {
    renderTL();
    const axis = screen.getByTestId('time-axis');
    expect(axis.querySelectorAll('text').length).toBeGreaterThan(0);
  });

  // --- Edges ---
  it('renders inferred edges dashed with a rule id', () => {
    const { container } = renderTL();
    const e = container.querySelector('[data-edge-id="e1"]') as SVGElement;
    expect(e).not.toBeNull();
    expect(e.getAttribute('data-origin')).toBe('inferred');
    expect(e.getAttribute('data-rule-id')).toBe('triggered_by_user_message@v1');
    expect(e.getAttribute('stroke-dasharray')).toBeTruthy();
  });

  it('exposes confidence on inferred edges (data-confidence + title)', () => {
    const { container } = renderTL();
    const e = container.querySelector('[data-edge-id="e1"]') as SVGElement;
    expect(e.getAttribute('data-confidence')).toBe('0.7');
    // Native tooltip carries both rule id and confidence
    const title = e.querySelector('title');
    expect(title?.textContent).toBe('triggered_by_user_message@v1 (0.7)');
  });

  it('renders a visible edge label with rule id + confidence when the edge is emphasized', () => {
    // Selecting node 'a' emphasizes incident edge e1 → its label becomes visible
    renderTL({ selectedNodeId: 'a' });
    const label = screen.getByTestId('edge-label');
    expect(label.textContent).toMatch(/triggered_by_user_message@v1/);
    expect(label.textContent).toMatch(/0\.7/);
  });

  it('does not render a visible edge label when no node is selected', () => {
    renderTL();
    expect(screen.queryByTestId('edge-label')).toBeNull();
  });

  it('renders deterministic edges as solid (no stroke-dasharray)', () => {
    const { container } = render(
      <Timeline
        nodes={nodes}
        edges={[deterministicEdge]}
        selectedNodeId={null}
        onSelect={() => {}}
        width={800}
        height={300}
      />
    );
    const e = container.querySelector('[data-edge-id="e2"]') as SVGElement;
    expect(e).not.toBeNull();
    expect(e.getAttribute('data-origin')).toBe('deterministic');
    const dash = e.getAttribute('stroke-dasharray');
    expect(!dash || dash === 'none' || dash === '').toBe(true);
  });

  it('renders edges with data-edge-id and data-origin attributes', () => {
    const { container } = renderTL();
    const e = container.querySelector('[data-edge-id="e1"]');
    expect(e?.getAttribute('data-origin')).toBe('inferred');
  });

  // --- Focus / selection ---
  it('marks the selected node with data-selected="true"', () => {
    const { container } = renderTL({ selectedNodeId: 'a' });
    expect(container.querySelector('[data-node-id="a"]')?.getAttribute('data-selected')).toBe('true');
  });

  it('marks the selected node and emphasizes its incident edges', () => {
    const { container } = renderTL({ selectedNodeId: 'a' });
    expect(container.querySelector('[data-node-id="a"]')?.getAttribute('data-selected')).toBe('true');
    expect(container.querySelector('[data-edge-id="e1"]')?.getAttribute('data-emphasized')).toBe('true');
  });

  it('dims non-incident edges when a node is selected', () => {
    const extraEdge = edge('e3', 'b', 'b', 'deterministic');
    const { container: c } = render(
      <Timeline
        nodes={nodes}
        edges={[...edges, extraEdge]}
        selectedNodeId={null}
        onSelect={() => {}}
        width={800}
        height={300}
      />
    );
    // Without selection, nothing is dimmed
    expect(c.querySelector('[data-edge-id="e1"]')?.getAttribute('data-dimmed')).not.toBe('true');
  });

  it('dims non-incident edges when a node is selected (part 2)', () => {
    // Add a non-incident edge: c→c is incident to 'c', not to 'a'
    const nodeC = node('c', 'tool_call', '2026-05-28T00:01:30Z', '2026-05-28T00:01:35Z');
    const edgeCtoC = edge('ecc', 'c', 'c', 'deterministic');
    const { container } = render(
      <Timeline
        nodes={[...nodes, nodeC]}
        edges={[...edges, edgeCtoC]}
        selectedNodeId={'a'}
        onSelect={() => {}}
        width={800}
        height={300}
      />
    );
    // e1 is incident to 'a' → emphasized
    expect(container.querySelector('[data-edge-id="e1"]')?.getAttribute('data-emphasized')).toBe('true');
    // ecc is NOT incident to 'a' → dimmed
    expect(container.querySelector('[data-edge-id="ecc"]')?.getAttribute('data-dimmed')).toBe('true');
  });

  // --- Click handlers ---
  it('fires onSelect with the node id on node click', () => {
    const onSelect = vi.fn();
    const { container } = renderTL({ onSelect });
    fireEvent.click(container.querySelector('[data-node-id="b"]')!);
    expect(onSelect).toHaveBeenCalledWith('b');
  });

  it('fires onSelect(null) when clicking the SVG background', () => {
    const onSelect = vi.fn();
    renderTL({ onSelect });
    // Click the main timeline canvas (not the minimap-track svg)
    const svg = screen.getByTestId('timeline-canvas');
    fireEvent.click(svg);
    expect(onSelect).toHaveBeenCalledWith(null);
  });

  // --- Zoom controls ---
  it('has zoom-in / zoom-out / fit controls', () => {
    renderTL();
    expect(screen.getByTestId('zoom-in')).toBeInTheDocument();
    expect(screen.getByTestId('zoom-out')).toBeInTheDocument();
    expect(screen.getByTestId('fit')).toBeInTheDocument();
  });

  it('zoom-in narrows the viewport (drops a node at the extent edge)', () => {
    const { container } = renderTL();
    // Default nodes: 'a' sits at the extent's start (t=0). On mount it is
    // visible; after a single zoom-in (window halves around center) the edge
    // node 'a' falls outside the viewport and is no longer rendered.
    expect(container.querySelector('[data-node-id="a"]')).not.toBeNull();
    fireEvent.click(screen.getByTestId('zoom-in'));
    expect(container.querySelector('[data-node-id="a"]')).toBeNull();
  });

  it('zoom-out broadens the viewport', () => {
    const { container } = renderTL();
    fireEvent.click(screen.getByTestId('zoom-out'));
    // After zoom-out the viewport is wider; both nodes should still render (clamp keeps them in)
    expect(container.querySelector('[data-node-id="a"]')).not.toBeNull();
    expect(container.querySelector('[data-node-id="b"]')).not.toBeNull();
  });

  it('fit resets the viewport to show all nodes', () => {
    const { container } = renderTL();
    fireEvent.click(screen.getByTestId('zoom-in'));
    fireEvent.click(screen.getByTestId('zoom-in'));
    fireEvent.click(screen.getByTestId('fit'));
    // After fit, both nodes should be visible
    expect(container.querySelector('[data-node-id="a"]')).not.toBeNull();
    expect(container.querySelector('[data-node-id="b"]')).not.toBeNull();
  });

  // --- Density cap ---
  it('renders aggregated marker when a lane exceeds MAX_NODES_PER_LANE', () => {
    // Spread 201 tool_call nodes over 201 minutes so all are in viewport
    const base = new Date('2026-05-28T00:00:00Z').getTime();
    const manyNodes: GraphNodeDto[] = Array.from({ length: 201 }, (_, i) =>
      node(`tool_${i}`, 'tool_call', new Date(base + i * 60_000).toISOString(), null)
    );
    const { container } = render(
      <Timeline
        nodes={manyNodes}
        edges={[]}
        selectedNodeId={null}
        onSelect={() => {}}
        width={800}
        height={300}
      />
    );
    expect(container.querySelector('[data-aggregated="true"]')).not.toBeNull();
  });

  // --- Tooltip ---
  it('shows a tooltip element when hovering a node', () => {
    const { container } = renderTL();
    const nodeEl = container.querySelector('[data-node-id="a"]')!;
    fireEvent.mouseEnter(nodeEl);
    // tooltip may be data-testid="node-tooltip"
    expect(screen.getByTestId('node-tooltip')).toBeInTheDocument();
  });

  it('hides the tooltip when mouse leaves', () => {
    const { container } = renderTL();
    const nodeEl = container.querySelector('[data-node-id="a"]')!;
    fireEvent.mouseEnter(nodeEl);
    fireEvent.mouseLeave(nodeEl);
    const tooltip = screen.queryByTestId('node-tooltip');
    // tooltip may hide by becoming invisible or removed
    const isHidden = !tooltip || tooltip.getAttribute('data-visible') === 'false' || (tooltip as HTMLElement).hidden;
    expect(isHidden).toBe(true);
  });

  it('tooltip shows nodeLabel primary for a tool_call node (not just raw node id)', () => {
    // node 'b' is tool_call with tool_name:'Read' → nodeLabel primary = 'Read'
    const bWithPayload = node('b', 'tool_call', '2026-05-28T00:01:00Z', '2026-05-28T00:01:05Z', { tool_name: 'Read', input: { file_path: '/a/x.jpg' } });
    const { container } = render(
      <Timeline
        nodes={[nodes[0], bWithPayload]}
        edges={edges}
        selectedNodeId={null}
        onSelect={() => {}}
        width={800}
        height={300}
      />
    );
    const nodeEl = container.querySelector('[data-node-id="b"]')!;
    fireEvent.mouseEnter(nodeEl);
    const tooltip = screen.getByTestId('node-tooltip');
    // primary label 'Read' must appear in the tooltip
    expect(tooltip.textContent).toContain('Read');
    // secondary (file basename) 'x.jpg' must appear
    expect(tooltip.textContent).toContain('x.jpg');
  });

  // --- Reduced-motion gating ---
  it('does not animate inferred edges when prefers-reduced-motion is set', () => {
    // Force window.matchMedia to report reduced-motion = true so useMediaQuery
    // returns true → reducedMotion = true → inferredEdge class is NOT applied.
    const mql = {
      matches: true,
      media: '(prefers-reduced-motion: reduce)',
      addEventListener: () => {},
      removeEventListener: () => {},
      addListener: () => {},
      removeListener: () => {},
      onchange: null,
      dispatchEvent: () => false,
    } as unknown as MediaQueryList;
    const spy = vi.spyOn(window, 'matchMedia').mockImplementation(() => mql);
    const { container } = renderTL();
    const inferred = container.querySelector('[data-edge-id="e1"]');
    expect(inferred).not.toBeNull();
    // Under reduced motion the inferredEdge animation class must be absent
    const className = inferred?.getAttribute('class') ?? '';
    expect(className).not.toContain('inferredEdge');
    spy.mockRestore();
  });

  // --- Wheel zoom ---
  it('attaches a non-passive wheel listener that calls preventDefault', () => {
    renderTL();
    // The timeline canvas SVG (distinct from the Minimap track SVG).
    const svg = screen.getByTestId('timeline-canvas');
    // Dispatch a real, cancelable WheelEvent on the canvas element. The
    // handler is attached imperatively with { passive: false }, so
    // preventDefault must take effect (defaultPrevented === true).
    const ev = new WheelEvent('wheel', { deltaY: -100, clientX: 400, bubbles: true, cancelable: true });
    svg.dispatchEvent(ev);
    expect(ev.defaultPrevented).toBe(true);
    // SVG is still rendered after the zoom.
    expect(screen.getByTestId('timeline-canvas')).toBeInTheDocument();
  });
});
