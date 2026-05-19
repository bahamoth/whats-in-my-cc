import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, screen, fireEvent, cleanup } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import { Timeline } from '../Timeline';
import type { GraphPayload } from '../../api/types';

const fixture: GraphPayload = {
  nodes: [
    { node_id: 'n1', schema_version: '1.0', session_id: 's', node_kind: 'user_message',
      started_at: '2026-05-19T10:00:00Z', ended_at: null,
      merge_keys: {}, source_event_ids: ['ev1'], source_uris: [], payload: {} },
    { node_id: 'n2', schema_version: '1.0', session_id: 's', node_kind: 'assistant_message',
      started_at: '2026-05-19T10:00:05Z', ended_at: null,
      merge_keys: {}, source_event_ids: ['ev2'], source_uris: [], payload: {} },
    { node_id: 'n3', schema_version: '1.0', session_id: 's', node_kind: 'tool_call',
      started_at: '2026-05-19T10:00:10Z', ended_at: null,
      merge_keys: {}, source_event_ids: ['ev3'], source_uris: [], payload: {} },
  ],
  edges: [
    { edge_id: 'e1', schema_version: '1.0', session_id: 's',
      from_node_id: 'n1', to_node_id: 'n2', edge_kind: 'message_reply',
      origin: 'deterministic', attributes: {} },
  ],
};

describe('Timeline', () => {
  afterEach(() => { cleanup(); });

  it('renders all six lanes', () => {
    render(<Timeline graph={fixture} selectedNodeId={null} onSelect={vi.fn()} />);
    for (const lane of ['Intent','Context','Action','State','OTel','Quality']) {
      expect(screen.getByText(lane)).toBeInTheDocument();
    }
  });

  it('draws one marker per drawable node', () => {
    const { container } = render(
      <Timeline graph={fixture} selectedNodeId={null} onSelect={vi.fn()} />
    );
    // each node marker has data-testid="node-marker"
    expect(container.querySelectorAll('[data-testid="node-marker"]').length).toBe(3);
  });

  it('shows placeholder text in empty lanes', () => {
    render(<Timeline graph={fixture} selectedNodeId={null} onSelect={vi.fn()} />);
    expect(screen.getByText(/no OTel observed/i)).toBeInTheDocument();
    expect(screen.getByText(/no findings yet/i)).toBeInTheDocument();
  });

  it('calls onSelect with node_id when a marker is clicked', () => {
    const onSelect = vi.fn();
    const { container } = render(
      <Timeline graph={fixture} selectedNodeId={null} onSelect={onSelect} />
    );
    const marker = container.querySelector('[data-node-id="n3"]');
    expect(marker).not.toBeNull();
    fireEvent.click(marker!);
    expect(onSelect).toHaveBeenCalledWith('n3');
  });

  it('renders one path per edge', () => {
    const { container } = render(
      <Timeline graph={fixture} selectedNodeId={null} onSelect={vi.fn()} />
    );
    expect(container.querySelectorAll('[data-testid="edge-path"]').length).toBe(1);
  });

  it('renders an otel_span node marker (regression for slice-3)', () => {
    const graph: GraphPayload = {
      nodes: [
        {
          node_id: 'nd_o_1',
          schema_version: '0.2.0',
          session_id: 's',
          node_kind: 'otel_span',
          started_at: '2026-05-19T00:00:00Z',
          ended_at: '2026-05-19T00:00:01Z',
          merge_keys: { trace_id: 't', span_id: 's' },
          source_event_ids: ['ev_o_1'],
          source_uris: [],
          payload: {},
        },
      ],
      edges: [],
    };
    render(<Timeline graph={graph} selectedNodeId={null} onSelect={() => {}} />);
    const marker = document.querySelector('[data-node-id="nd_o_1"]');
    expect(marker).not.toBeNull();
    expect(screen.getByText('OTel')).toBeInTheDocument();
    expect(screen.queryByText(/no OTel observed/i)).toBeNull();
  });
});
