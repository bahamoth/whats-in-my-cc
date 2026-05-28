/**
 * PR-4 RED — Waterfall replaces the legacy SVG Timeline as the primary
 * replay canvas. Interface contract (drop-in for Timeline so
 * SessionDetailPage swaps painlessly):
 *
 *   props: { graph, selectedNodeId, onSelect, onZoomChange?, zoom? }
 *
 * DOM contract:
 *   - role="img", aria-label="session timeline"
 *   - All 8 lanes always rendered with their label text ("Intent" …).
 *   - Each drawable node: <element data-node-id="<id>" data-node-kind="<kind>" />
 *   - x-position monotonically increases with started_at within the session.
 *   - When ended_at > started_at, node is rendered as a rectangle whose width
 *     reflects duration; otherwise as a point (data-shape="point").
 *   - Clicking a node fires onSelect(nodeId).
 *
 * Visual extras (brush-zoom, virtualization) are deferred to a follow-up.
 */
import { describe, expect, it, vi } from 'vitest';
import { render, fireEvent, screen } from '@testing-library/react';
import { Waterfall } from '../Waterfall';
import type { GraphPayload } from '../../../api/types';

function n(id: string, kind: string, startISO: string, endISO?: string) {
  return {
    node_id: id,
    schema_version: 'v1',
    session_id: 's',
    node_kind: kind,
    started_at: startISO,
    ended_at: endISO ?? null,
    merge_keys: {},
    source_event_ids: [`ev-${id}`],
    source_uris: [],
    payload: {},
  };
}

const fixture: GraphPayload = {
  nodes: [
    n('n1', 'user_message', '2026-05-19T10:00:00Z'),
    n('n2', 'assistant_message', '2026-05-19T10:00:05Z'),
    n('n3', 'tool_call', '2026-05-19T10:00:10Z', '2026-05-19T10:00:13Z'), // 3s
    n('n4', 'otel_span', '2026-05-19T10:00:11Z', '2026-05-19T10:00:11.5Z'), // 500ms
  ],
  edges: [
    {
      edge_id: 'e1',
      schema_version: 'v1',
      session_id: 's',
      from_node_id: 'n1',
      to_node_id: 'n2',
      edge_kind: 'message_reply',
      origin: 'deterministic',
      attributes: {},
    },
  ],
};

describe('Waterfall', () => {
  it('renders an aria-labelled timeline region', () => {
    render(<Waterfall graph={fixture} selectedNodeId={null} onSelect={vi.fn()} />);
    expect(screen.getByRole('img', { name: /session timeline/i })).toBeInTheDocument();
  });

  it('renders every lane label even when empty', () => {
    render(<Waterfall graph={fixture} selectedNodeId={null} onSelect={vi.fn()} />);
    for (const lane of ['Intent', 'Context', 'Action', 'State', 'Files', 'Hook', 'OTel', 'Quality']) {
      expect(screen.getByText(lane)).toBeInTheDocument();
    }
  });

  it('renders one drawable node per fixture row', () => {
    const { container } = render(
      <Waterfall graph={fixture} selectedNodeId={null} onSelect={vi.fn()} />,
    );
    expect(container.querySelectorAll('[data-node-id]').length).toBe(4);
  });

  it('places nodes monotonically in x by started_at within their lane', () => {
    const { container } = render(
      <Waterfall graph={fixture} selectedNodeId={null} onSelect={vi.fn()} />,
    );
    const n1 = container.querySelector('[data-node-id="n1"]') as SVGElement;
    const n2 = container.querySelector('[data-node-id="n2"]') as SVGElement;
    expect(n1).not.toBeNull();
    expect(n2).not.toBeNull();
    const x1 = parseFloat(n1.getAttribute('x') ?? n1.getAttribute('cx') ?? '0');
    const x2 = parseFloat(n2.getAttribute('x') ?? n2.getAttribute('cx') ?? '0');
    expect(x2).toBeGreaterThan(x1);
  });

  it('renders nodes with duration as rectangles whose width reflects the span', () => {
    const { container } = render(
      <Waterfall graph={fixture} selectedNodeId={null} onSelect={vi.fn()} />,
    );
    const n3 = container.querySelector('[data-node-id="n3"]') as SVGElement; // 3s
    const n4 = container.querySelector('[data-node-id="n4"]') as SVGElement; // 0.5s
    expect(n3).not.toBeNull();
    expect(n4).not.toBeNull();
    expect(n3.getAttribute('data-shape')).toBe('bar');
    expect(n4.getAttribute('data-shape')).toBe('bar');
    const w3 = parseFloat(n3.getAttribute('width') ?? '0');
    const w4 = parseFloat(n4.getAttribute('width') ?? '0');
    expect(w3).toBeGreaterThan(w4);
  });

  it('renders durationless nodes as points', () => {
    const { container } = render(
      <Waterfall graph={fixture} selectedNodeId={null} onSelect={vi.fn()} />,
    );
    const n1 = container.querySelector('[data-node-id="n1"]') as SVGElement;
    expect(n1.getAttribute('data-shape')).toBe('point');
  });

  it('clicking a node fires onSelect with the node id', () => {
    const onSelect = vi.fn();
    const { container } = render(
      <Waterfall graph={fixture} selectedNodeId={null} onSelect={onSelect} />,
    );
    const n3 = container.querySelector('[data-node-id="n3"]') as SVGElement;
    fireEvent.click(n3);
    expect(onSelect).toHaveBeenCalledWith('n3');
  });

  it('marks the selected node with data-selected="true"', () => {
    const { container } = render(
      <Waterfall graph={fixture} selectedNodeId="n2" onSelect={vi.fn()} />,
    );
    expect(container.querySelector('[data-node-id="n2"]')?.getAttribute('data-selected')).toBe('true');
    expect(container.querySelector('[data-node-id="n1"]')?.getAttribute('data-selected')).toBe('false');
  });

  it('node carries data-node-kind so token styling can hook (no hard-coded colours)', () => {
    const { container } = render(
      <Waterfall graph={fixture} selectedNodeId={null} onSelect={vi.fn()} />,
    );
    expect(container.querySelector('[data-node-id="n3"]')?.getAttribute('data-node-kind')).toBe('tool_call');
    expect(container.querySelector('[data-node-id="n4"]')?.getAttribute('data-node-kind')).toBe('otel_span');
  });

  it('empty graph still renders all lane labels without crashing', () => {
    render(<Waterfall graph={{ nodes: [], edges: [] }} selectedNodeId={null} onSelect={vi.fn()} />);
    for (const lane of ['Intent', 'Context', 'Action', 'State', 'Files', 'Hook', 'OTel', 'Quality']) {
      expect(screen.getByText(lane)).toBeInTheDocument();
    }
  });
});
