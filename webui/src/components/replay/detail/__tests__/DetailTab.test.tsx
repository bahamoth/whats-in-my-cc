// webui/src/components/replay/detail/__tests__/DetailTab.test.tsx
/**
 * R3 RED — DetailTab renders human-readable fields and a token badge from
 * the raw record's usage. Plan R3 Task 3 / spec §4.
 */
import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { DetailTab } from '../DetailTab';

const node = {
  node_kind: 'assistant_message',
  started_at: '2026-05-28T09:14:08Z',
  ended_at: '2026-05-28T09:14:10Z',
} as any;

describe('DetailTab', () => {
  it('shows the node kind and timestamp', () => {
    render(<DetailTab node={node} record={null} episodePhase={null} />);
    expect(screen.getByText('assistant_message')).toBeInTheDocument();
  });

  it('shows a token badge when usage is present in the raw record', () => {
    render(<DetailTab node={node} record={{ message: { usage: { output_tokens: 451, input_tokens: 6, cache_read_input_tokens: 59055 } } }} episodePhase={null} />);
    expect(screen.getByText(/451/)).toBeInTheDocument();
    expect(screen.getByText(/out/i)).toBeInTheDocument();
  });

  it('shows tool name and error state for a tool node record', () => {
    render(<DetailTab node={{ node_kind: 'tool_call', started_at: node.started_at, ended_at: null } as any} record={{ tool_result: { is_error: true, content: 'boom' } }} episodePhase="repair" />);
    expect(screen.getByText('tool_call')).toBeInTheDocument();
    expect(screen.getByText('repair')).toBeInTheDocument();
    expect(screen.getByText(/error/i)).toBeInTheDocument();
  });

  it('shows an empty hint with no node', () => {
    render(<DetailTab node={null} record={null} episodePhase={null} />);
    expect(screen.getByText(/select a node/i)).toBeInTheDocument();
  });
});
