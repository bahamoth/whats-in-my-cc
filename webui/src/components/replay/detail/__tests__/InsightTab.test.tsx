// webui/src/components/replay/detail/__tests__/InsightTab.test.tsx
/**
 * S6.2 — InsightTab now renders FocusedInsightGraph above NodeDetail (which
 * owns findings rendering). Old inline findings list removed.
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

function n(id: string, kind = 'tool_call', payload: unknown = {}): GraphNodeDto {
  return { node_id: id, schema_version: '1', session_id: 's', node_kind: kind, started_at: '', ended_at: null, merge_keys: {}, source_event_ids: [], source_uris: [], payload };
}
function e(id: string, from: string, to: string): GraphEdgeDto {
  return { edge_id: id, schema_version: '1', session_id: 's', from_node_id: from, to_node_id: to, edge_kind: 'x', origin: 'deterministic', attributes: {}, inference_rule_id: null, confidence: null };
}

const toolNode = n('b', 'tool_call', { tool_name: 'Bash', input: { command: 'ls' } });

describe('InsightTab', () => {
  it('renders findings via NodeDetail (summary, category, severity visible)', () => {
    render(
      <InsightTab findings={[finding({})]} nodes={[toolNode]} edges={[]} selectedNodeId="b"
        onSelectNode={() => {}} node={toolNode} record={null} episodePhase={null} />,
    );
    expect(screen.getByText('risky rm -rf')).toBeInTheDocument();
    expect(screen.getByText(/risky_action/)).toBeInTheDocument();
    expect(screen.getByText(/high/i)).toBeInTheDocument();
  });

  it('renders confidence as a percentage via NodeDetail', () => {
    render(
      <InsightTab findings={[finding({ confidence: 0.8 })]} nodes={[toolNode]} edges={[]} selectedNodeId="b"
        onSelectNode={() => {}} node={toolNode} record={null} episodePhase={null} />,
    );
    expect(screen.getByText('80%')).toBeInTheDocument();
  });

  it('shows an empty hint when no node and no findings', () => {
    render(
      <InsightTab findings={[]} nodes={[]} edges={[]} selectedNodeId={null}
        onSelectNode={() => {}} node={null} record={null} episodePhase={null} />,
    );
    expect(screen.getByText(/no insights|no findings/i)).toBeInTheDocument();
  });

  it('renders the focused subgraph above NodeDetail for the selected node', () => {
    // ResizeObserver is mocked globally in webui/src/test/setup.ts — no per-file mock needed.
    const nodes = [n('a'), toolNode, n('c')];
    const edges = [e('e1', 'a', 'b'), e('e2', 'b', 'c')];
    render(
      <InsightTab findings={[]} nodes={nodes} edges={edges} selectedNodeId="b"
        onSelectNode={() => {}} node={toolNode} record={null} episodePhase={null} />,
    );
    expect(screen.getByTestId('focused-graph')).toBeInTheDocument();
  });

  it('findings render via NodeDetail when a node is selected and there are findings', () => {
    const nodes = [n('a'), toolNode, n('c')];
    const edges = [e('e1', 'a', 'b')];
    render(
      <InsightTab findings={[finding({})]} nodes={nodes} edges={edges} selectedNodeId="b"
        onSelectNode={() => {}} node={toolNode} record={null} episodePhase={null} />,
    );
    expect(screen.getByTestId('focused-graph')).toBeInTheDocument();
    expect(screen.getByText('risky rm -rf')).toBeInTheDocument();
  });

  it('renders NodeDetail label (tool name) when a node is provided', () => {
    render(
      <InsightTab findings={[]} nodes={[toolNode]} edges={[]} selectedNodeId="b"
        onSelectNode={() => {}} node={toolNode} record={null} episodePhase={null} />,
    );
    // 'Bash' now appears both in NodeDetail and in the focused subgraph node label
    // (nodeLabel primary for tool_call with tool_name:'Bash'). At least one must be present.
    expect(screen.getAllByText('Bash').length).toBeGreaterThan(0);
  });
});
