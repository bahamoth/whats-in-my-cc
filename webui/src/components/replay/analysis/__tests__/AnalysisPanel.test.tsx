import { render, screen } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import { describe, expect, test } from 'vitest';
import { AnalysisPanel } from '../AnalysisPanel';
import type { SessionMetricsDto } from '../../../../api/types';

const m: SessionMetricsDto = {
  session_id: 's1',
  tool_call_total: 10,
  tool_failure_count: 2,
  tool_failure_rate: 0.2,
  verification_total: 4,
  verification_passed: 3,
  verification_pass_rate: 0.75,
  context_bloat_count: 1,
  cache_hit_ratio: 0.6,
  detector_firing: { tool_failure: 2, context_bloat: 1 },
};

describe('AnalysisPanel', () => {
  test('renders rates and detector distribution', () => {
    render(<AnalysisPanel metrics={m} />);
    expect(screen.getByText(/20%/)).toBeInTheDocument();       // tool_failure_rate
    expect(screen.getByText(/75%/)).toBeInTheDocument();       // verification_pass_rate
    expect(screen.getByText(/tool_failure/)).toBeInTheDocument(); // detector dist
  });

  test('renders tool failure count', () => {
    render(<AnalysisPanel metrics={m} />);
    expect(screen.getByText(/2\/10/)).toBeInTheDocument();     // tool_failure_count / tool_call_total
  });

  test('renders cache hit ratio', () => {
    render(<AnalysisPanel metrics={m} />);
    expect(screen.getByText(/60%/)).toBeInTheDocument();       // cache_hit_ratio
  });

  test('renders context bloat count', () => {
    render(<AnalysisPanel metrics={m} />);
    // context_bloat_count=1 rendered as standalone "1" in the count cell
    expect(screen.getAllByText('1').length).toBeGreaterThan(0); // context_bloat_count
  });

  test('renders detector_firing distribution labels', () => {
    render(<AnalysisPanel metrics={m} />);
    expect(screen.getByText(/context_bloat/)).toBeInTheDocument();
  });

  test('empty state when null', () => {
    render(<AnalysisPanel metrics={null} />);
    expect(screen.getByText(/분석할 지표가 없|no metrics/i)).toBeInTheDocument();
  });
});
