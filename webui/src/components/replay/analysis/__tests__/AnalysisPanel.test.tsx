import { render, screen } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import { describe, expect, test } from 'vitest';
import { AnalysisPanel } from '../AnalysisPanel';
import type { SessionMetricsDto } from '../../../../api/types';

const m: SessionMetricsDto = {
  session_id: 's1',
  tool_call_total: 10,
  tool_failure_count: 2,
  verification_total: 4,
  verification_passed: 3,
  verification_failed: 1,
  verification_unknown: 0,
  context_bloat_count: 1,
  tool_user_rejected: 0,
  tool_policy_denied: 0,
  tool_cancelled: 0,
  tool_backgrounded: 0,
  detector_firing: { tool_failure: 2, context_bloat: 1 },
};

describe('AnalysisPanel', () => {
  test('renders rates computed from counts and detector distribution', () => {
    render(<AnalysisPanel metrics={m} />);
    // tool_failure_count(2)/tool_call_total(10) → 20%
    expect(screen.getByText(/20%/)).toBeInTheDocument();
    // verification_passed(3)/(passed(3)+failed(1)) → 75%
    expect(screen.getByText(/75%/)).toBeInTheDocument();
    expect(screen.getByText(/tool_failure/)).toBeInTheDocument(); // detector dist
  });

  test('renders tool failure count', () => {
    render(<AnalysisPanel metrics={m} />);
    expect(screen.getByText(/2\/10/)).toBeInTheDocument();     // tool_failure_count / tool_call_total
  });

  test('does NOT render cache_hit_ratio row (removed from /metrics)', () => {
    render(<AnalysisPanel metrics={m} />);
    expect(screen.queryByText(/캐시 히트율/)).not.toBeInTheDocument();
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

  test('shows verification_unknown when > 0', () => {
    const mWithUnknown: SessionMetricsDto = { ...m, verification_unknown: 2 };
    render(<AnalysisPanel metrics={mWithUnknown} />);
    expect(screen.getByText(/미측정 2/)).toBeInTheDocument();
  });

  test('shows 측정 없음 when measured denominator is zero', () => {
    const mNone: SessionMetricsDto = {
      ...m, verification_passed: 0, verification_failed: 0, verification_unknown: 0,
    };
    render(<AnalysisPanel metrics={mNone} />);
    expect(screen.getByText(/측정 없음/)).toBeInTheDocument();
  });

  test('empty state when null', () => {
    render(<AnalysisPanel metrics={null} />);
    expect(screen.getByText(/분석할 지표가 없|no metrics/i)).toBeInTheDocument();
  });
});
