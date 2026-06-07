// webui/src/components/replay/detail/__tests__/InsightTab.test.tsx
/**
 * InsightTab 5-layer skeleton: H(header+badge+chips) → ①WHAT → ②HOW(metrics)
 * → ③SIGNALS. Fully event-driven (no graph node).
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

describe('InsightTab 5-layer skeleton', () => {
  const toolEvent = ev('b', 'tool_call', { tool_name: 'Bash', input: { command: 'cargo test' } });
  const toolMetrics = {
    durationMs: 57, success: true, decisionSource: 'config', decisionType: 'accept',
    inputBytes: 362, resultBytes: 302, sequence: 763,
  };
  const matchedResult = {
    ...ev('r', 'tool_result', { tool_result: { content: '142 passed', is_error: false } }),
    tool_use_id: null,
  } as ObservedEventDto;

  it('(H) shows provenance badge: 원본 for native tool_call', () => {
    render(
      <InsightTab signals={[]} event={toolEvent} toolMetrics={toolMetrics} llmMetrics={null} matchedResult={null} />,
    );
    expect(screen.getByText('원본')).toBeInTheDocument();
  });

  it('(H) shows 가공 badge for derived diff_hunk event', () => {
    const diffEv = ev('d', 'diff_hunk', { file_path: 'src/lib.rs', patch_preview: '@@ -1 +1 @@' });
    render(
      <InsightTab signals={[]} event={diffEv} toolMetrics={null} llmMetrics={null} matchedResult={null} />,
    );
    expect(screen.getByText('가공')).toBeInTheDocument();
  });

  it('(①) WHAT section title is rendered', () => {
    render(
      <InsightTab signals={[]} event={toolEvent} toolMetrics={toolMetrics} llmMetrics={null} matchedResult={null} />,
    );
    expect(screen.getByText(/What — 한 일/i)).toBeInTheDocument();
  });

  it('(①) WHAT shows tool_call command from WhatSection', () => {
    render(
      <InsightTab signals={[]} event={toolEvent} toolMetrics={toolMetrics} llmMetrics={null} matchedResult={null} />,
    );
    expect(screen.getByText(/cargo test/)).toBeInTheDocument();
  });

  it('(①) WHAT shows matched tool_result output', () => {
    render(
      <InsightTab signals={[]} event={toolEvent} toolMetrics={toolMetrics} llmMetrics={null} matchedResult={matchedResult} />,
    );
    expect(screen.getByText(/142 passed/)).toBeInTheDocument();
  });

  it('(①) WHAT shows prompt for user_message', () => {
    const userEv = ev('u', 'user_message', { content: '리팩터링 해주세요' });
    render(
      <InsightTab signals={[]} event={userEv} toolMetrics={null} llmMetrics={null} matchedResult={null} />,
    );
    expect(screen.getByText(/리팩터링 해주세요/)).toBeInTheDocument();
  });

  it('(②) HOW section title is rendered', () => {
    render(
      <InsightTab signals={[]} event={toolEvent} toolMetrics={toolMetrics} llmMetrics={null} matchedResult={null} />,
    );
    expect(screen.getByText(/How — 지표/i)).toBeInTheDocument();
  });

  it('(③) SIGNALS section title rendered when signals present', () => {
    const s: SignalDto = {
      signal_id: 'sig1', schema_version: '1', session_id: 's', detector: 'tool_failure',
      subkind: null, summary: 'failed', evidence_refs: [], facts: {}, provenance: {}, created_at: '',
    };
    render(
      <InsightTab signals={[s]} event={toolEvent} toolMetrics={toolMetrics} llmMetrics={null} matchedResult={null} />,
    );
    expect(screen.getByText('Signals')).toBeInTheDocument();
  });

  it('does not show old nodeSecondary one-liner (WHAT supersedes it)', () => {
    // The header should NOT contain the tool arg summary as a nodeSecondary span —
    // it now lives in the WHAT section instead.
    render(
      <InsightTab signals={[]} event={toolEvent} toolMetrics={toolMetrics} llmMetrics={null} matchedResult={null} />,
    );
    // The WHAT section will show "cargo test", but the header should not have it as a separate secondary span.
    // We verify by checking there's no element with class nodeSecondary (it was removed from InsightTab).
    // Simplest check: 'cargo test' appears exactly once (in WhatSection, not also in the header).
    const matches = screen.getAllByText(/cargo test/);
    expect(matches.length).toBe(1);
  });
});
