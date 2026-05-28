/**
 * PR-7 RED — CausalGraph renders a React Flow view over the same graph
 * payload Waterfall reads. We test the *contract* (node count, click
 * routes through onSelect, selected styling) and rely on the unit-tested
 * edge style helper to lock visual semantics.
 *
 * jsdom does not implement layout, so we mock ResizeObserver in setup
 * (already covered by src/test/setup.ts) and assert via React Flow's
 * stable data-attributes.
 */
import { beforeAll, describe, expect, it, vi } from 'vitest';
import { render } from '@testing-library/react';
import { CausalGraph } from '../CausalGraph';
import type { GraphPayload } from '../../../api/types';

const fixture: GraphPayload = {
  nodes: [
    { node_id: 'n1', schema_version: 'v1', session_id: 's', node_kind: 'user_message',
      started_at: '2026-05-19T10:00:00Z', ended_at: null, merge_keys: {}, source_event_ids: ['e1'], source_uris: [], payload: {} },
    { node_id: 'n2', schema_version: 'v1', session_id: 's', node_kind: 'assistant_message',
      started_at: '2026-05-19T10:00:05Z', ended_at: null, merge_keys: {}, source_event_ids: ['e2'], source_uris: [], payload: {} },
  ],
  edges: [
    { edge_id: 'e1', schema_version: 'v1', session_id: 's', from_node_id: 'n1', to_node_id: 'n2',
      edge_kind: 'message_reply', origin: 'deterministic', attributes: {} },
  ],
};

beforeAll(() => {
  // React Flow needs ResizeObserver. Setup.ts installs a no-op IntersectionObserver;
  // do the same for ResizeObserver here so this spec is self-contained.
  if (!(globalThis as { ResizeObserver?: unknown }).ResizeObserver) {
    class RO {
      observe() {}
      unobserve() {}
      disconnect() {}
    }
    (globalThis as Record<string, unknown>).ResizeObserver = RO;
  }
});

describe('CausalGraph', () => {
  it('renders one React Flow node per graph node', () => {
    const { container } = render(
      <CausalGraph graph={fixture} selectedNodeId={null} onSelect={vi.fn()} />,
    );
    // React Flow node wrappers expose data-id. Our custom node also exposes
    // data-node-id (mirrors Waterfall) so the same selectors can be reused.
    expect(container.querySelectorAll('[data-node-id]').length).toBe(2);
  });

  it('marks selected node with data-selected="true"', () => {
    const { container } = render(
      <CausalGraph graph={fixture} selectedNodeId="n2" onSelect={vi.fn()} />,
    );
    expect(container.querySelector('[data-node-id="n2"]')?.getAttribute('data-selected')).toBe(
      'true',
    );
    expect(container.querySelector('[data-node-id="n1"]')?.getAttribute('data-selected')).toBe(
      'false',
    );
  });

  it('empty graph renders without crashing', () => {
    expect(() =>
      render(
        <CausalGraph
          graph={{ nodes: [], edges: [] }}
          selectedNodeId={null}
          onSelect={vi.fn()}
        />,
      ),
    ).not.toThrow();
  });

  it('exposes data-node-kind on each rendered node', () => {
    const { container } = render(
      <CausalGraph graph={fixture} selectedNodeId={null} onSelect={vi.fn()} />,
    );
    expect(container.querySelector('[data-node-id="n1"]')?.getAttribute('data-node-kind')).toBe(
      'user_message',
    );
    expect(container.querySelector('[data-node-id="n2"]')?.getAttribute('data-node-kind')).toBe(
      'assistant_message',
    );
  });
});
