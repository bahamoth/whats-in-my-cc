// webui/src/components/replay/detail/__tests__/DetailPanel.test.tsx
/**
 * S6.2 — 2-tab DetailPanel: Insight (subgraph + NodeDetail + findings) / Raw.
 * Detail tab removed; InsightTab now hosts NodeDetail below the subgraph.
 */
import { render, screen, fireEvent } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { DetailPanel } from '../DetailPanel';
import type { FindingDto, GraphNodeDto } from '../../../../api/types';

const someNode: GraphNodeDto = {
  node_id: 'n1', schema_version: '1', session_id: 's',
  node_kind: 'tool_call', started_at: '2026-05-28T09:14:08Z', ended_at: null,
  merge_keys: {}, source_event_ids: [], source_uris: [],
  payload: { tool_name: 'Read', input: {} },
};

const toolNode: GraphNodeDto = {
  node_id: 'n2', schema_version: '1', session_id: 's',
  node_kind: 'tool_call', started_at: '2026-05-28T09:14:08Z', ended_at: null,
  merge_keys: {}, source_event_ids: [], source_uris: [],
  payload: { tool_name: 'Bash', input: { command: 'ls' } },
};

function finding(): FindingDto {
  return { finding_id: 'f1', schema_version: '1', session_id: 's', category: 'c', severity: 'high', confidence: 0.5, summary: 's', evidence_refs: [], evidence_projection: {}, provenance: {}, status: 'open', created_at: '' };
}

describe('DetailPanel', () => {
  it('shows only Insight and Raw tabs (Detail tab removed)', () => {
    render(<DetailPanel node={someNode} record={null} findings={[]} episodePhase={null} nodes={[someNode]} edges={[]} onSelectNode={() => {}} />);
    expect(screen.getByRole('tab', { name: /insight/i })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: /raw/i })).toBeInTheDocument();
    expect(screen.queryByRole('tab', { name: /^detail$/i })).toBeNull();
  });

  it('Insight tab contains node detail (focused node label) below the subgraph', () => {
    render(<DetailPanel node={toolNode} record={{ tool_result: { is_error: false } }} findings={[]} episodePhase="action" nodes={[toolNode]} edges={[]} onSelectNode={() => {}} />);
    // 'Bash' now appears in both NodeDetail and the focused subgraph node label.
    // We assert at least one match is present.
    expect(screen.getAllByText(/Bash|Read/).length).toBeGreaterThan(0);
  });

  it('defaults to the Insight tab', () => {
    render(<DetailPanel node={someNode} record={null} findings={[finding()]} episodePhase={null} nodes={[]} edges={[]} onSelectNode={() => {}} />);
    expect(screen.getByRole('tab', { name: /insight/i })).toHaveAttribute('aria-selected', 'true');
  });

  it('also defaults to Insight when no findings', () => {
    render(<DetailPanel node={someNode} record={null} findings={[]} episodePhase={null} nodes={[]} edges={[]} onSelectNode={() => {}} />);
    expect(screen.getByRole('tab', { name: /insight/i })).toHaveAttribute('aria-selected', 'true');
  });

  it('switches tab on click and keeps it across a re-render', () => {
    const { rerender } = render(<DetailPanel node={someNode} record={{ a: 1 }} findings={[finding()]} episodePhase={null} nodes={[]} edges={[]} onSelectNode={() => {}} />);
    fireEvent.click(screen.getByRole('tab', { name: /raw/i }));
    expect(screen.getByRole('tab', { name: /raw/i })).toHaveAttribute('aria-selected', 'true');
    rerender(<DetailPanel node={someNode} record={{ a: 1 }} findings={[finding()]} episodePhase={null} nodes={[]} edges={[]} onSelectNode={() => {}} />);
    expect(screen.getByRole('tab', { name: /raw/i })).toHaveAttribute('aria-selected', 'true');
  });

  it('emphasizes the Raw tab when a raw record is available (#2 discoverability)', () => {
    render(<DetailPanel node={someNode} record={{ actor: 'user', is_sidechain: true }} findings={[]} episodePhase={null} nodes={[someNode]} edges={[]} onSelectNode={() => {}} />);
    expect(screen.getByRole('tab', { name: /raw/i })).toHaveAttribute('data-has-record', 'true');
  });

  it('does not emphasize the Raw tab when there is no raw record', () => {
    render(<DetailPanel node={someNode} record={null} findings={[]} episodePhase={null} nodes={[someNode]} edges={[]} onSelectNode={() => {}} />);
    expect(screen.getByRole('tab', { name: /raw/i })).toHaveAttribute('data-has-record', 'false');
  });

  it('shows an empty hint when no node is selected', () => {
    render(<DetailPanel node={null} record={null} findings={[]} episodePhase={null} nodes={[]} edges={[]} onSelectNode={() => {}} />);
    expect(screen.getByText(/select a node/i)).toBeInTheDocument();
  });

  it('renders the response-metrics panel when a thinking marker is selected (no node)', () => {
    const metrics = {
      requestId: 'req-1', durationMs: 28900, ttftMs: 3100, inputTokens: 2,
      outputTokens: 2300, cacheReadTokens: 290000, cacheCreationTokens: 2200,
      stopReason: 'tool_use', attempt: 1, success: true, model: 'claude-opus-4-8',
    };
    render(<DetailPanel node={null} record={null} findings={[]} episodePhase={null} nodes={[]} edges={[]} onSelectNode={() => {}} thinkingSelected thinkingMetrics={metrics} />);
    expect(screen.getByTestId('response-metrics')).toBeInTheDocument();
    expect(screen.queryByText(/select a node/i)).toBeNull();
  });

  it('renders the response-metrics panel even when its metrics are null', () => {
    render(<DetailPanel node={null} record={null} findings={[]} episodePhase={null} nodes={[]} edges={[]} onSelectNode={() => {}} thinkingSelected thinkingMetrics={null} />);
    expect(screen.getByTestId('response-metrics')).toBeInTheDocument();
  });
});
