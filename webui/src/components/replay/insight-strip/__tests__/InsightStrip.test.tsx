import { describe, expect, it } from 'vitest';
import { render, screen, fireEvent, within } from '@testing-library/react';
import { InsightStrip } from '../InsightStrip';
import type { SessionUsageDto, VerificationRunDto, SignalDto } from '../../../../api/types';

const usage: SessionUsageDto = {
  session_id: 's1', turns: 5, input_tokens: 200_000,
  cache_creation_input_tokens: 3_900_000, cache_read_input_tokens: 199_500_000,
  output_tokens: 1_300_000, billed_tokens: 5_400_000, cache_hit_ratio: 0.98,
  estimated_cost_usd: 102.5, cost_basis: 'estimate_public_pricing',
  pricing_version: 'v1', models_without_pricing: [],
  by_model: [{
    model: 'claude-opus-4-8', turns: 5,
    input_tokens: 200_000, cache_creation_input_tokens: 3_900_000,
    cache_read_input_tokens: 199_500_000, output_tokens: 1_300_000,
    estimated_cost_usd: 102.5, priced: true,
  }],
};

function vr(kind: string, status: string): VerificationRunDto {
  return {
    verification_run_id: `vr_${kind}`, schema_version: '1', session_id: 's1',
    source: 'transcript_bash', command: kind, command_kind: kind,
    trigger_event_id: 'e', trigger_tool_use_id: null, status,
    detection_basis: 'known_tool', status_basis: 'exit',
    started_at: '2026-05-30T00:00:00Z', ended_at: null, exit_code: null,
    failure_summary: null, covered_diff_hunk_ids: [],
  };
}

function sig(detector: string, subkind: string | null = null, summary = 'test'): SignalDto {
  return {
    signal_id: `sig_${detector}`, schema_version: '1', session_id: 's1',
    detector, subkind, summary, evidence_refs: ['ev1'], facts: {}, provenance: {}, created_at: '',
  };
}

describe('InsightStrip', () => {
  it('renders the five redesigned cards and NONE of the removed tiles', () => {
    render(<InsightStrip usage={usage} verificationRuns={[]} signals={[]} />);
    expect(screen.getByTestId('insight-card-context')).toBeInTheDocument();
    expect(screen.getByTestId('insight-card-tokens')).toBeInTheDocument();
    expect(screen.getByTestId('insight-card-verification')).toBeInTheDocument();
    expect(screen.getByTestId('insight-card-tool_failure')).toBeInTheDocument();
    expect(screen.getByTestId('insight-card-cost')).toBeInTheDocument();
    // removed tiles (spec §1/§5/§11 P1)
    expect(screen.queryByTestId('kpi-risk')).toBeNull();
    expect(screen.queryByTestId('kpi-episodes')).toBeNull();
    expect(screen.queryByTestId('kpi-outcome')).toBeNull();
    expect(screen.queryByTestId('kpi-latency')).toBeNull();
  });

  it('shows the cache-hit value and a 측정 badge on the context card', () => {
    render(<InsightStrip usage={usage} verificationRuns={[]} signals={[]} />);
    const card = screen.getByTestId('insight-card-context');
    expect(within(card).getByText('98%')).toBeInTheDocument();
    expect(within(card).getByTestId('provenance-badge')).toHaveTextContent('측정');
  });

  it('badges 미수집·예정 when usage is absent (slice not yet wired)', () => {
    render(<InsightStrip usage={undefined} verificationRuns={[]} signals={[]} />);
    const card = screen.getByTestId('insight-card-context');
    expect(within(card).getByTestId('provenance-badge')).toHaveTextContent('미수집·예정');
  });

  it('expands a card on click to show its drill lines, and collapses on second click', () => {
    render(
      <InsightStrip usage={usage} verificationRuns={[vr('test_suite_rust', 'passed')]} signals={[]} />,
    );
    expect(screen.queryByTestId('insight-drill-verification')).toBeNull();
    fireEvent.click(screen.getByTestId('insight-card-verification-toggle'));
    const drill = screen.getByTestId('insight-drill-verification');
    expect(within(drill).getByText(/test_suite_rust → passed/)).toBeInTheDocument();
    fireEvent.click(screen.getByTestId('insight-card-verification-toggle'));
    expect(screen.queryByTestId('insight-drill-verification')).toBeNull();
  });

  it('is single-open — expanding one card closes the previously open one', () => {
    render(<InsightStrip usage={usage} verificationRuns={[]} signals={[]} />);
    fireEvent.click(screen.getByTestId('insight-card-context-toggle'));
    expect(screen.getByTestId('insight-drill-context')).toBeInTheDocument();
    fireEvent.click(screen.getByTestId('insight-card-tokens-toggle'));
    expect(screen.queryByTestId('insight-drill-context')).toBeNull();
    expect(screen.getByTestId('insight-drill-tokens')).toBeInTheDocument();
  });

  it('clicking the ? tooltip does NOT expand the card', () => {
    render(<InsightStrip usage={usage} verificationRuns={[]} signals={[]} />);
    const card = screen.getByTestId('insight-card-context');
    fireEvent.click(within(card).getByTestId('infotip-trigger'));
    expect(screen.queryByTestId('insight-drill-context')).toBeNull();
    // the tooltip itself opened
    expect(within(card).getByRole('tooltip')).toBeInTheDocument();
  });

  it('tool_failure card counts signals with detector=tool_failure', () => {
    render(
      <InsightStrip
        usage={undefined}
        verificationRuns={[]}
        signals={[
          sig('tool_failure', 'non_zero_exit', 'exit 1'),
          sig('context_bloat', null, 'not a failure'),
          sig('tool_failure', null, 'permission denied'),
        ]}
      />,
    );
    const card = screen.getByTestId('insight-card-tool_failure');
    expect(within(card).getByText('2')).toBeInTheDocument();
    // measured provenance (deterministic count)
    expect(card.getAttribute('data-provenance')).toBe('measured');
  });
});
