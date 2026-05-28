/**
 * PR-3 RED — KpiStrip. Compact top-of-page summary the user reads in the
 * first ~2 seconds. Sources for PR-3:
 *  - outcome: derived from findings (severity counts)
 *  - verification coverage: covered_hunks / total_hunks
 *  - episode count: episodes.length
 *  - risk count: high-severity findings count
 *  - cost / latency: placeholder "—" (metric endpoint pending)
 *
 * The placeholder ensures no `NaN` or `undefined%` ever reaches the DOM.
 */
import { describe, expect, it } from 'vitest';
import { render, screen, within } from '@testing-library/react';
import { KpiStrip } from '../KpiStrip';

const baseProps = {
  outcome: 'clean' as const,
  verificationCoverage: { covered: 4, total: 5 },
  episodeCount: 3,
  riskCount: 1,
  cost: undefined,
  latencyP95Ms: undefined,
};

describe('KpiStrip', () => {
  it('renders every KPI tile with a testid', () => {
    render(<KpiStrip {...baseProps} />);
    expect(screen.getByTestId('kpi-outcome')).toBeInTheDocument();
    expect(screen.getByTestId('kpi-verification')).toBeInTheDocument();
    expect(screen.getByTestId('kpi-episodes')).toBeInTheDocument();
    expect(screen.getByTestId('kpi-risk')).toBeInTheDocument();
    expect(screen.getByTestId('kpi-cost')).toBeInTheDocument();
    expect(screen.getByTestId('kpi-latency')).toBeInTheDocument();
  });

  it('shows verification coverage as a percentage', () => {
    render(<KpiStrip {...baseProps} />);
    const tile = screen.getByTestId('kpi-verification');
    expect(within(tile).getByText('80%')).toBeInTheDocument();
  });

  it('shows episode count', () => {
    render(<KpiStrip {...baseProps} />);
    const tile = screen.getByTestId('kpi-episodes');
    expect(within(tile).getByText('3')).toBeInTheDocument();
  });

  it('shows risk count and colour-codes via aria-label when > 0', () => {
    render(<KpiStrip {...baseProps} />);
    const tile = screen.getByTestId('kpi-risk');
    expect(within(tile).getByText('1')).toBeInTheDocument();
    expect(tile.getAttribute('aria-label') ?? '').toMatch(/risk/i);
  });

  it('uses "—" placeholder when cost / latency are unavailable', () => {
    render(<KpiStrip {...baseProps} />);
    expect(within(screen.getByTestId('kpi-cost')).getByText('—')).toBeInTheDocument();
    expect(within(screen.getByTestId('kpi-latency')).getByText('—')).toBeInTheDocument();
  });

  it('renders cost / latency when supplied (formatted)', () => {
    render(<KpiStrip {...baseProps} cost={0.0042} latencyP95Ms={1500} />);
    expect(within(screen.getByTestId('kpi-cost')).getByText('$0.0042')).toBeInTheDocument();
    expect(within(screen.getByTestId('kpi-latency')).getByText('1.5s')).toBeInTheDocument();
  });

  it('verification tile gracefully shows "—" when total is 0', () => {
    render(<KpiStrip {...baseProps} verificationCoverage={{ covered: 0, total: 0 }} />);
    expect(within(screen.getByTestId('kpi-verification')).getByText('—')).toBeInTheDocument();
  });

  it('outcome variants reflect in tile data-state attribute', () => {
    const { rerender } = render(<KpiStrip {...baseProps} outcome="clean" />);
    expect(screen.getByTestId('kpi-outcome').dataset.state).toBe('clean');
    rerender(<KpiStrip {...baseProps} outcome="attention" />);
    expect(screen.getByTestId('kpi-outcome').dataset.state).toBe('attention');
    rerender(<KpiStrip {...baseProps} outcome="problem" />);
    expect(screen.getByTestId('kpi-outcome').dataset.state).toBe('problem');
  });
});
