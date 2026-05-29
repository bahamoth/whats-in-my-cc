// webui/src/components/replay/detail/__tests__/InsightTab.test.tsx
/**
 * R3 RED — InsightTab lists findings linked to the selected node. Absorbs the
 * WhyPanel. Plan R3 Task 4 / spec §4.
 * R5 — extended: InsightTab now mounts FocusedInsightGraph above the findings
 * list. Plan R5 Task 3 / spec §6.
 */
import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { InsightTab } from '../InsightTab';
import type { FindingDto, GraphNodeDto, GraphEdgeDto } from '../../../../api/types';

function finding(p: Partial<FindingDto>): FindingDto {
  return {
    finding_id: 'f1', schema_version: '1', session_id: 's', category: 'risky_action',
    severity: 'high', confidence: 0.8, summary: 'risky rm -rf', evidence_refs: [],
    evidence_projection: {}, provenance: {}, status: 'open', created_at: '', ...p,
  };
}

function n(id: string): GraphNodeDto {
  return { node_id: id, schema_version: '1', session_id: 's', node_kind: 'tool_call', started_at: '', ended_at: null, merge_keys: {}, source_event_ids: [], source_uris: [], payload: {} };
}
function e(id: string, from: string, to: string): GraphEdgeDto {
  return { edge_id: id, schema_version: '1', session_id: 's', from_node_id: from, to_node_id: to, edge_kind: 'x', origin: 'deterministic', attributes: {}, inference_rule_id: null, confidence: null };
}

describe('InsightTab', () => {
  it('renders each finding with summary, category, and severity', () => {
    render(<InsightTab findings={[finding({})]} nodes={[]} edges={[]} selectedNodeId={null} onSelectNode={() => {}} />);
    expect(screen.getByText('risky rm -rf')).toBeInTheDocument();
    expect(screen.getByText(/risky_action/)).toBeInTheDocument();
    expect(screen.getByText(/high/i)).toBeInTheDocument();
  });

  it('renders confidence as a percentage', () => {
    render(<InsightTab findings={[finding({ confidence: 0.8 })]} nodes={[]} edges={[]} selectedNodeId={null} onSelectNode={() => {}} />);
    expect(screen.getByText('80%')).toBeInTheDocument();
  });

  it('shows an empty hint when the node has no findings', () => {
    render(<InsightTab findings={[]} nodes={[]} edges={[]} selectedNodeId={null} onSelectNode={() => {}} />);
    expect(screen.getByText(/no insights|no findings/i)).toBeInTheDocument();
  });

  it('renders the focused subgraph above the findings for the selected node', () => {
    // ResizeObserver is mocked globally in webui/src/test/setup.ts — no per-file mock needed.
    const nodes = [n('a'), n('b'), n('c')];
    const edges = [e('e1', 'a', 'b'), e('e2', 'b', 'c')];
    render(
      <InsightTab
        findings={[]}
        nodes={nodes}
        edges={edges}
        selectedNodeId="b"
        onSelectNode={() => {}}
      />,
    );
    expect(screen.getByTestId('focused-graph')).toBeInTheDocument();
  });

  it('findings still render when a node is selected and there are findings', () => {
    const nodes = [n('a'), n('b'), n('c')];
    const edges = [e('e1', 'a', 'b')];
    render(
      <InsightTab
        findings={[finding({})]}
        nodes={nodes}
        edges={edges}
        selectedNodeId="b"
        onSelectNode={() => {}}
      />,
    );
    expect(screen.getByTestId('focused-graph')).toBeInTheDocument();
    expect(screen.getByText('risky rm -rf')).toBeInTheDocument();
  });
});
