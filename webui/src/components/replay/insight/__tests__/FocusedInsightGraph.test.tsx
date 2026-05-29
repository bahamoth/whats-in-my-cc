// webui/src/components/replay/insight/__tests__/FocusedInsightGraph.test.tsx
/** R5 RED — FocusedInsightGraph renders the bounded neighborhood. Plan R5 Task 2 / spec §6. */
import { render, screen, fireEvent } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { FocusedInsightGraph } from '../FocusedInsightGraph';
import type { GraphNodeDto, GraphEdgeDto } from '../../../../api/types';

function n(id: string, kind = 'tool_call', payload: unknown = {}): GraphNodeDto {
  return { node_id: id, schema_version: '1', session_id: 's', node_kind: kind, started_at: '', ended_at: null, merge_keys: {}, source_event_ids: [], source_uris: [], payload };
}
function e(id: string, from: string, to: string, origin = 'deterministic'): GraphEdgeDto {
  return { edge_id: id, schema_version: '1', session_id: 's', from_node_id: from, to_node_id: to, edge_kind: 'x', origin, attributes: {}, inference_rule_id: origin === 'inferred' ? 'caused_repair@v1' : null, confidence: origin === 'inferred' ? 0.6 : null };
}
// node 'b' is a tool_call with tool_name:'Bash' so nodeLabel yields primary='Bash'
const nodes = [
  n('a', 'user_message'),
  n('b', 'tool_call', { tool_name: 'Bash', input: { command: 'ls -la' } }),
  n('c'),
];
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

  it('marks exactly the selected node as the neighborhood center', () => {
    const { container } = render(
      <FocusedInsightGraph nodes={nodes} edges={edges} selectedNodeId="b" onSelectNode={() => {}} />,
    );
    // The selected node b is the single center; a and c are present but not.
    expect(container.querySelectorAll('[data-center="true"]')).toHaveLength(1);
    expect(container.querySelectorAll('[data-center="false"]')).toHaveLength(2);
    const center = container.querySelector('[data-center="true"]');
    // node b is a tool_call with tool_name:'Bash' → nodeLabel primary = 'Bash'
    // The hash id must NOT be the main text; 'Bash' must appear.
    expect(center?.textContent).toContain('Bash');
  });

  it('renders nodeLabel primary+secondary in node body (not raw hash id as headline)', () => {
    const { container } = render(
      <FocusedInsightGraph nodes={nodes} edges={edges} selectedNodeId="b" onSelectNode={() => {}} />,
    );
    const center = container.querySelector('[data-center="true"]');
    // primary label 'Bash' present
    expect(center?.textContent).toContain('Bash');
    // secondary arg 'ls -la' present (truncated, but full value fits here)
    expect(center?.textContent).toContain('ls -la');
    // The 6-char hash tail of 'b' (which is just 'b' itself) must not be the
    // prominent text — we verify by checking 'Bash' is present instead.
    // (We don't forbid the id appearing in a de-emphasised tag, but it is no
    //  longer the *only* label.)
  });
  // NOTE: edge styling/labels (inferred → dashed + rule-id label) are not
  // asserted here — @xyflow/react does not render edges in jsdom (no node
  // measurements). The style mapping is locked at the unit level in
  // causalEdgeStyle.test.ts, and buildLayout wires label = inference_rule_id.
});
