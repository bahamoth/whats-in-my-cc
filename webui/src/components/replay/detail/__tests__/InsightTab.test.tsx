// webui/src/components/replay/detail/__tests__/InsightTab.test.tsx
/**
 * Metrics-led Insight tab — now EVENT-first (no graph node): a compact header
 * (from the event's kind + payload) + EntityMetricsPanel + that event's
 * Findings. Drives entirely off an ObservedEvent.
 */
import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { InsightTab } from '../InsightTab';
import type { FindingDto, ObservedEventDto } from '../../../../api/types';

function finding(p: Partial<FindingDto>): FindingDto {
  return {
    finding_id: 'f1', schema_version: '1', session_id: 's', category: 'risky_action',
    severity: 'high', confidence: 0.8, summary: 'risky rm -rf', evidence_refs: [],
    evidence_projection: {}, provenance: {}, status: 'open', created_at: '', ...p,
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
  it('renders findings (summary, category, severity visible)', () => {
    render(
      <InsightTab findings={[finding({})]} event={toolEvent} toolMetrics={toolMetrics} llmMetrics={null} />,
    );
    expect(screen.getByText('risky rm -rf')).toBeInTheDocument();
    expect(screen.getByText(/risky_action/)).toBeInTheDocument();
    expect(screen.getByText(/high/i)).toBeInTheDocument();
  });

  it('renders confidence as a percentage', () => {
    render(
      <InsightTab findings={[finding({ confidence: 0.8 })]} event={toolEvent} toolMetrics={toolMetrics} llmMetrics={null} />,
    );
    expect(screen.getByText('80%')).toBeInTheDocument();
  });

  it('shows an empty hint when no event and no findings', () => {
    render(<InsightTab findings={[]} event={null} toolMetrics={null} llmMetrics={null} />);
    expect(screen.getByText(/no insights|no findings/i)).toBeInTheDocument();
  });

  it('renders the header (tool name) when an event is provided', () => {
    render(<InsightTab findings={[]} event={toolEvent} toolMetrics={toolMetrics} llmMetrics={null} />);
    expect(screen.getByText('Bash')).toBeInTheDocument();
  });

  it('shows what a tool call did (operation summary) in the header', () => {
    const computer = ev('c', 'tool_call', {
      tool_name: 'mcp__claude-in-chrome__computer',
      input: { action: 'left_click', coordinate: [638, 220] },
    });
    render(<InsightTab findings={[]} event={computer} toolMetrics={toolMetrics} llmMetrics={null} />);
    expect(screen.getByText('left_click (638, 220)')).toBeInTheDocument();
  });

  it('renders the entity metrics panel for the selected event', () => {
    render(<InsightTab findings={[]} event={toolEvent} toolMetrics={toolMetrics} llmMetrics={null} />);
    expect(screen.getByTestId('entity-metrics')).toBeInTheDocument();
    expect(screen.getByText(/결정 출처/)).toBeInTheDocument();
  });
});
