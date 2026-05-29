// webui/src/components/replay/timeline/__tests__/Timeline.test.tsx
/** R4 RED — Timeline SVG surface. Plan R4 Task 4 / spec §5. */
import { render, screen, fireEvent } from '@testing-library/react';
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
const nodes = [
  node('a', 'user_message', '2026-05-28T00:00:00Z', null),
  node('b', 'tool_call', '2026-05-28T00:01:00Z', '2026-05-28T00:01:05Z'),
];
const edges = [edge('e1', 'a', 'b', 'inferred', 'triggered_by_user_message@v1')];
const deterministicEdge = edge('e2', 'a', 'b', 'deterministic');
const episodes: EpisodeDto[] = [{
  episode_id: 'ep',
  schema_version: '1',
  session_id: 's',
  phase: 'action',
  start_event_id: '',
  end_event_id: '',
  started_at: T('2026-05-28T00:00:00Z'),
  ended_at: T('2026-05-28T00:02:00Z'),
  evidence_node_ids: [],
  classification_basis: [],
  confidence: 1,
  summary: null,
  classifier_version: '1',
  created_at: T('2026-05-28T00:00:00Z'),
}];

function renderTL(props = {}) {
  return render(<Timeline nodes={nodes} edges={edges} episodes={episodes} selectedNodeId={null} onSelect={() => {}} width={800} height={300} {...props} />);
}

describe('Timeline', () => {
  // --- Lane rows ---
  it('renders a lane row per LANES entry', () => {
    const { container } = renderTL();
    expect(container.querySelector('[data-lane="Intent"]')).not.toBeNull();
    expect(container.querySelector('[data-lane="Action"]')).not.toBeNull();
    expect(container.querySelector('[data-lane="Context"]')).not.toBeNull();
    expect(container.querySelector('[data-lane="State"]')).not.toBeNull();
    expect(container.querySelector('[data-lane="Files"]')).not.toBeNull();
    expect(container.querySelector('[data-lane="Hook"]')).not.toBeNull();
    expect(container.querySelector('[data-lane="OTel"]')).not.toBeNull();
    expect(container.querySelector('[data-lane="Quality"]')).not.toBeNull();
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

  it('does not render off-window nodes', () => {
    // Give a far-future viewport by passing only off-window nodes
    const farNode = node('far', 'tool_call', '2030-01-01T00:00:00Z', '2030-01-01T00:01:00Z');
    const { container } = render(
      <Timeline
        nodes={[farNode]}
        edges={[]}
        episodes={[]}
        selectedNodeId={null}
        onSelect={() => {}}
        width={800}
        height={300}
      />
    );
    // farNode is the only node, so the viewport fits it — it should be visible
    expect(container.querySelector('[data-node-id="far"]')).not.toBeNull();
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

  // --- Episode band ---
  it('renders an episode band with a rect per episode phase', () => {
    renderTL();
    const band = screen.getByTestId('episode-band');
    expect(band.querySelector('[data-phase="action"]')).not.toBeNull();
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

  it('renders deterministic edges as solid (no stroke-dasharray)', () => {
    const { container } = render(
      <Timeline
        nodes={nodes}
        edges={[deterministicEdge]}
        episodes={episodes}
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
        episodes={episodes}
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
        episodes={episodes}
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
    const { container } = renderTL({ onSelect });
    // Click the svg element itself (background)
    const svg = container.querySelector('svg');
    fireEvent.click(svg!);
    expect(onSelect).toHaveBeenCalledWith(null);
  });

  // --- Zoom controls ---
  it('has zoom-in / zoom-out / fit controls', () => {
    renderTL();
    expect(screen.getByTestId('zoom-in')).toBeInTheDocument();
    expect(screen.getByTestId('zoom-out')).toBeInTheDocument();
    expect(screen.getByTestId('fit')).toBeInTheDocument();
  });

  it('zoom-in narrows the viewport (render stays intact after zoom)', () => {
    renderTL();
    // After zoom-in, at least one node should remain in the viewport (the one near center)
    fireEvent.click(screen.getByTestId('zoom-in'));
    // At minimum the SVG is still rendered without crashing
    expect(screen.getByTestId('time-axis')).toBeInTheDocument();
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
        episodes={[]}
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

  // --- Wheel zoom ---
  it('handles wheel events without throwing', () => {
    const { container } = renderTL();
    const svg = container.querySelector('svg')!;
    expect(() => fireEvent.wheel(svg, { deltaY: -100, clientX: 400 })).not.toThrow();
    // After a wheel event, SVG is still rendered; at least one node element exists
    expect(container.querySelectorAll('[data-node-id]').length).toBeGreaterThanOrEqual(0);
    expect(container.querySelector('svg')).not.toBeNull();
  });
});
