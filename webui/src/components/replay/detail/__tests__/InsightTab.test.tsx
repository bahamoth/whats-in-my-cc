// webui/src/components/replay/detail/__tests__/InsightTab.test.tsx
/**
 * Metrics-led Insight tab — EVENT-first (no graph node): a compact header
 * (from the event's kind + payload) + EntityMetricsPanel + that event's
 * Signals. Drives entirely off an ObservedEvent.
 */
import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { InsightTab } from '../InsightTab';
import type { SignalDto, ObservedEventDto } from '../../../../api/types';

function signal(p: Partial<SignalDto>): SignalDto {
  return {
    signal_id: 'sig1', schema_version: '1', session_id: 's', detector: 'tool_failure',
    subkind: null, summary: 'command exited with code 1', evidence_refs: ['ev1'],
    facts: {}, provenance: {}, created_at: '', ...p,
  };
}

function ev(id: string, kind = 'tool_call', payload: unknown = {}): ObservedEventDto {
  return {
    event_id: id, raw_event_id: '', session_id: 's', event_uuid: null, parent_uuid: null,
    observed_at: '', actor: 'assistant', kind, subkind: null, tool_use_id: null, tool_name: null,
    turn_id: null, is_sidechain: false, is_meta: false, payload,
  } as ObservedEventDto;
}

const toolEvent = ev('b', 'tool_call', { tool_name: 'Bash', input: { command: 'ls' } });

const toolMetrics = {
  durationMs: 57, success: true, decisionSource: 'config', decisionType: 'accept',
  inputBytes: 362, resultBytes: 302, sequence: 763,
};

describe('InsightTab', () => {
  it('renders signals (summary and detector visible; no severity/confidence)', () => {
    render(
      <InsightTab signals={[signal({})]} event={toolEvent} toolMetrics={toolMetrics} llmMetrics={null} />,
    );
    expect(screen.getByText('command exited with code 1')).toBeInTheDocument();
    expect(screen.getByText(/tool_failure/)).toBeInTheDocument();
    // severity and confidence must NOT appear (signal has neither)
    expect(screen.queryByText(/high|medium|low/i)).toBeNull();
    expect(screen.queryByText(/%/)).toBeNull();
  });

  it('renders optional subkind when present', () => {
    render(
      <InsightTab
        signals={[signal({ subkind: 'non_zero_exit' })]}
        event={toolEvent}
        toolMetrics={toolMetrics}
        llmMetrics={null}
      />,
    );
    expect(screen.getByText('non_zero_exit')).toBeInTheDocument();
  });

  it('shows an empty hint when no event and no signals', () => {
    render(<InsightTab signals={[]} event={null} toolMetrics={null} llmMetrics={null} />);
    expect(screen.getByText(/no insights/i)).toBeInTheDocument();
  });

  it('renders the header (tool name) when an event is provided', () => {
    render(<InsightTab signals={[]} event={toolEvent} toolMetrics={toolMetrics} llmMetrics={null} />);
    expect(screen.getByText('Bash')).toBeInTheDocument();
  });

  it('shows what a tool call did (operation summary) in the header', () => {
    const computer = ev('c', 'tool_call', {
      tool_name: 'mcp__claude-in-chrome__computer',
      input: { action: 'left_click', coordinate: [638, 220] },
    });
    render(<InsightTab signals={[]} event={computer} toolMetrics={toolMetrics} llmMetrics={null} />);
    expect(screen.getByText('left_click (638, 220)')).toBeInTheDocument();
  });

  it('renders the entity metrics panel for the selected event', () => {
    render(<InsightTab signals={[]} event={toolEvent} toolMetrics={toolMetrics} llmMetrics={null} />);
    expect(screen.getByTestId('entity-metrics')).toBeInTheDocument();
    expect(screen.getByText(/결정 출처/)).toBeInTheDocument();
  });

  it('renders the Signals section title when signals are present', () => {
    render(
      <InsightTab signals={[signal({})]} event={toolEvent} toolMetrics={toolMetrics} llmMetrics={null} />,
    );
    expect(screen.getByText('Signals')).toBeInTheDocument();
  });
});
