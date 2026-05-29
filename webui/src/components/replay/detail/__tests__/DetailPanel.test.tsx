// webui/src/components/replay/detail/__tests__/DetailPanel.test.tsx
/**
 * R3 RED — DetailPanel hosts Insight/Detail/Raw tabs, defaults to Insight
 * (Detail when no findings), and keeps the chosen tab across re-render.
 * Plan R3 Task 5 / spec §4.
 */
import { render, screen, fireEvent } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { DetailPanel } from '../DetailPanel';
import type { FindingDto, GraphNodeDto } from '../../../api/types';

const node = { node_kind: 'tool_call', started_at: '2026-05-28T09:14:08Z', ended_at: null } as GraphNodeDto;
function finding(): FindingDto {
  return { finding_id: 'f1', schema_version: '1', session_id: 's', category: 'c', severity: 'high', confidence: 0.5, summary: 's', evidence_refs: [], evidence_projection: {}, provenance: {}, status: 'open', created_at: '' };
}

describe('DetailPanel', () => {
  it('renders three tabs', () => {
    render(<DetailPanel node={node} record={null} findings={[]} episodePhase={null} />);
    expect(screen.getByRole('tab', { name: /insight/i })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: /detail/i })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: /raw/i })).toBeInTheDocument();
  });

  it('defaults to the Insight tab when there are findings', () => {
    render(<DetailPanel node={node} record={null} findings={[finding()]} episodePhase={null} />);
    expect(screen.getByRole('tab', { name: /insight/i })).toHaveAttribute('aria-selected', 'true');
  });

  it('defaults to Detail when the node has no findings', () => {
    render(<DetailPanel node={node} record={null} findings={[]} episodePhase={null} />);
    expect(screen.getByRole('tab', { name: /detail/i })).toHaveAttribute('aria-selected', 'true');
  });

  it('switches tab on click and keeps it across a re-render', () => {
    const { rerender } = render(<DetailPanel node={node} record={{ a: 1 }} findings={[finding()]} episodePhase={null} />);
    fireEvent.click(screen.getByRole('tab', { name: /raw/i }));
    expect(screen.getByRole('tab', { name: /raw/i })).toHaveAttribute('aria-selected', 'true');
    rerender(<DetailPanel node={node} record={{ a: 1 }} findings={[finding()]} episodePhase={null} />);
    expect(screen.getByRole('tab', { name: /raw/i })).toHaveAttribute('aria-selected', 'true');
  });

  it('shows an empty hint when no node is selected', () => {
    render(<DetailPanel node={null} record={null} findings={[]} episodePhase={null} />);
    expect(screen.getByText(/select a node/i)).toBeInTheDocument();
  });
});
