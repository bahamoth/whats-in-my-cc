// webui/src/components/replay/detail/__tests__/DetailPanel.test.tsx
/**
 * 2-tab DetailPanel: Insight / Raw — EVENT-first (no graph node). Selecting
 * any ObservedEvent (incl. thinking) drives the panel; the Insight tab hosts a
 * header + EntityMetricsPanel + Signals, the Raw tab the source record/blocks.
 */
import { render, screen, fireEvent } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { DetailPanel } from '../DetailPanel';
import type { SignalDto, ObservedEventDto } from '../../../../api/types';

function ev(id: string, kind = 'tool_call', payload: unknown = {}): ObservedEventDto {
  return {
    event_id: id, raw_event_id: '', session_id: 's', event_uuid: null, parent_uuid: null,
    observed_at: '2026-05-28T09:14:08Z', actor: 'assistant', kind, subkind: null,
    tool_use_id: null, tool_name: null, turn_id: null, is_sidechain: false, is_meta: false, payload,
  } as ObservedEventDto;
}

const someEvent = ev('n1', 'tool_call', { tool_name: 'Read', input: {} });
const toolEvent = ev('n2', 'tool_call', { tool_name: 'Bash', input: { command: 'ls' } });

function signal(): SignalDto {
  return {
    signal_id: 'sig1', schema_version: '1', session_id: 's', detector: 'tool_failure',
    subkind: null, summary: 'exit 1', evidence_refs: ['n1'], facts: {}, provenance: {}, created_at: '',
  };
}

describe('DetailPanel', () => {
  it('shows only Insight and Raw tabs (Detail tab removed)', () => {
    render(<DetailPanel event={someEvent} record={null} signals={[]} />);
    expect(screen.getByRole('tab', { name: /insight/i })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: /raw/i })).toBeInTheDocument();
    expect(screen.queryByRole('tab', { name: /^detail$/i })).toBeNull();
  });

  it('Insight tab shows the header label', () => {
    render(<DetailPanel event={toolEvent} record={{ tool_result: { is_error: false } }} signals={[]} />);
    expect(screen.getAllByText(/Bash|Read/).length).toBeGreaterThan(0);
  });

  it('defaults to the Insight tab', () => {
    render(<DetailPanel event={someEvent} record={null} signals={[signal()]} />);
    expect(screen.getByRole('tab', { name: /insight/i })).toHaveAttribute('aria-selected', 'true');
  });

  it('switches tab on click and keeps it across a re-render', () => {
    const { rerender } = render(<DetailPanel event={someEvent} record={{ a: 1 }} signals={[signal()]} />);
    fireEvent.click(screen.getByRole('tab', { name: /raw/i }));
    expect(screen.getByRole('tab', { name: /raw/i })).toHaveAttribute('aria-selected', 'true');
    rerender(<DetailPanel event={someEvent} record={{ a: 1 }} signals={[signal()]} />);
    expect(screen.getByRole('tab', { name: /raw/i })).toHaveAttribute('aria-selected', 'true');
  });

  it('emphasizes the Raw tab when a raw record is available (#2 discoverability)', () => {
    render(<DetailPanel event={someEvent} record={{ actor: 'user', is_sidechain: true }} signals={[]} />);
    expect(screen.getByRole('tab', { name: /raw/i })).toHaveAttribute('data-has-record', 'true');
  });

  it('does not emphasize the Raw tab when there is no raw record', () => {
    render(<DetailPanel event={someEvent} record={null} signals={[]} />);
    expect(screen.getByRole('tab', { name: /raw/i })).toHaveAttribute('data-has-record', 'false');
  });

  it('emphasizes the Raw tab when only rawBlocks are available (no record)', () => {
    render(
      <DetailPanel
        event={someEvent}
        record={null}
        signals={[]}
        rawBlocks={[{ source: 'transcript', label: 'tool_call', record: { tool_name: 'Bash' } }]}
      />,
    );
    expect(screen.getByRole('tab', { name: /raw/i })).toHaveAttribute('data-has-record', 'true');
  });

  it('shows an empty hint when no event is selected', () => {
    render(<DetailPanel event={null} record={null} signals={[]} />);
    expect(screen.getByText(/select an event/i)).toBeInTheDocument();
  });

  it('renders a thinking event as a normal event in the Insight tab (no special-case)', () => {
    const thinking = ev('t1', 'thinking', { signature: 'sig' });
    const metrics = {
      requestId: 'req-1', durationMs: 28900, ttftMs: 3100, inputTokens: 2,
      outputTokens: 2300, cacheReadTokens: 290000, cacheCreationTokens: 2200,
      stopReason: 'tool_use', attempt: 1, success: true, model: 'claude-opus-4-8', costUsd: null,
    };
    render(<DetailPanel event={thinking} record={null} signals={[]} llmMetrics={metrics} />);
    expect(screen.getByTestId('entity-metrics')).toBeInTheDocument();
    expect(screen.queryByText(/select an event/i)).toBeNull();
  });
});
