// webui/src/components/replay/detail/__tests__/InsightTab.test.tsx
/**
 * Task 6 — metrics-led Insight tab: a compact node header + EntityMetricsPanel
 * (kind-dependent collected metrics) + that node's Findings. The old
 * FocusedInsightGraph subgraph and shallow per-kind sections were removed.
 */
import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { InsightTab } from '../InsightTab';
import type { FindingDto, GraphNodeDto } from '../../../../api/types';

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

const toolNode = n('b', 'tool_call', { tool_name: 'Bash', input: { command: 'ls' } });

const toolMetrics = {
  durationMs: 57, success: true, decisionSource: 'config', decisionType: 'accept',
  inputBytes: 362, resultBytes: 302, sequence: 763,
};

describe('InsightTab', () => {
  it('renders findings (summary, category, severity visible)', () => {
    render(
      <InsightTab findings={[finding({})]} node={toolNode} toolMetrics={toolMetrics} llmMetrics={null} />,
    );
    expect(screen.getByText('risky rm -rf')).toBeInTheDocument();
    expect(screen.getByText(/risky_action/)).toBeInTheDocument();
    expect(screen.getByText(/high/i)).toBeInTheDocument();
  });

  it('renders confidence as a percentage', () => {
    render(
      <InsightTab findings={[finding({ confidence: 0.8 })]} node={toolNode} toolMetrics={toolMetrics} llmMetrics={null} />,
    );
    expect(screen.getByText('80%')).toBeInTheDocument();
  });

  it('shows an empty hint when no node and no findings', () => {
    render(<InsightTab findings={[]} node={null} toolMetrics={null} llmMetrics={null} />);
    expect(screen.getByText(/no insights|no findings/i)).toBeInTheDocument();
  });

  it('renders the node header (tool name) when a node is provided', () => {
    render(<InsightTab findings={[]} node={toolNode} toolMetrics={toolMetrics} llmMetrics={null} />);
    expect(screen.getByText('Bash')).toBeInTheDocument();
  });

  it('renders the entity metrics panel for the selected node', () => {
    render(<InsightTab findings={[]} node={toolNode} toolMetrics={toolMetrics} llmMetrics={null} />);
    expect(screen.getByTestId('entity-metrics')).toBeInTheDocument();
    // tool-execution metric meaning is surfaced
    expect(screen.getByText(/결정 출처/)).toBeInTheDocument();
  });

  it('does not render the focused subgraph (removed)', () => {
    render(<InsightTab findings={[]} node={toolNode} toolMetrics={toolMetrics} llmMetrics={null} />);
    expect(screen.queryByTestId('focused-graph')).toBeNull();
  });
});
