import { screen, fireEvent } from '@testing-library/react';
import { renderWithI18n as render } from '../../../../test/i18nRender';
import '@testing-library/jest-dom/vitest';
import { describe, expect, test, vi } from 'vitest';
import { AnalysisPanel } from '../AnalysisPanel';
import type { SessionMetricsDto, SignalDto, VerificationRunDto } from '../../../../api/types';

const m: SessionMetricsDto = {
  session_id: 's1',
  tool_call_total: 10,
  tool_failure_count: 2,
  verification_total: 4,
  verification_passed: 3,
  verification_failed: 1,
  verification_unknown: 0,
  verification_not_executed: 0,
  context_bloat_count: 1,
  tool_user_rejected: 0,
  tool_policy_denied: 0,
  tool_cancelled: 0,
  tool_backgrounded: 0,
  turn_duration_ms_total: 0,
  turn_duration_count: 0,
  api_error_count: 0,
  api_rate_limit_count: 0,
  input_tokens: 0,
  output_tokens: 0,
  cache_read_input_tokens: 0,
  cache_creation_input_tokens: 0,
  estimated_cost_usd: 0,
  compact_boundary_count: 0,
  tool_result_truncated_count: 0,
  user_interruption_count: 0,
  detector_firing: { tool_failure: 2, context_bloat: 1 },
  llm_request_p50: {
    ttft_ms: { p50: null, n: 0 },
    duration_ms: { p50: null, n: 0 },
    output_tokens: { p50: null, n: 0 },
    cost_usd: { p50: null, n: 0 },
  },
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

  // --- drill-down (dogfooding 2026-06-11): detector bars expand to their signals,
  // each linking to its evidence event ---
  const reReadSignal: SignalDto = {
    signal_id: 'sig_rr1',
    schema_version: 'signal.v1',
    session_id: 's1',
    detector: 're_read',
    subkind: null,
    summary: 'File src/big.rs read 5 times (re-read, context-loss signal).',
    evidence_refs: ['ev_read_1'],
    facts: { file_path: 'src/big.rs', read_count: 5 },
    provenance: {},
    created_at: '',
  };
  const mWithReRead: SessionMetricsDto = {
    ...m,
    detector_firing: { re_read: 1, tool_failure: 2 },
  };

  test('expanding a detector row reveals its signals (file_path + read_count)', () => {
    render(<AnalysisPanel metrics={mWithReRead} signals={[reReadSignal]} />);
    // Collapsed: signal detail not shown yet.
    expect(screen.queryByText(/src\/big\.rs/)).not.toBeInTheDocument();
    // Expand the re_read row.
    fireEvent.click(screen.getByRole('button', { name: /re_read/ }));
    expect(screen.getByText(/src\/big\.rs/)).toBeInTheDocument();
    expect(screen.getByText(/5회/)).toBeInTheDocument(); // read_count
  });

  test('clicking a drilled signal selects its evidence event', () => {
    const onSelectEvent = vi.fn();
    render(
      <AnalysisPanel
        metrics={mWithReRead}
        signals={[reReadSignal]}
        onSelectEvent={onSelectEvent}
      />,
    );
    fireEvent.click(screen.getByRole('button', { name: /re_read/ }));
    fireEvent.click(screen.getByText(/src\/big\.rs/));
    expect(onSelectEvent).toHaveBeenCalledWith('ev_read_1');
  });
});

function mkRun(over: Partial<VerificationRunDto>): VerificationRunDto {
  return {
    verification_run_id: 'vr1',
    schema_version: 'verification_run.v1',
    session_id: 's1',
    source: 'bash',
    command: 'cargo test',
    command_kind: 'test_suite_rust',
    trigger_event_id: 'ev_t1',
    trigger_tool_use_id: null,
    status: 'passed',
    status_provenance: 'measured',
    detection_basis: 'known_tool',
    status_basis: 'exit',
    started_at: '2026-06-10T02:30:00+00:00',
    ended_at: null,
    exit_code: 0,
    failure_summary: null,
    covered_diff_hunk_ids: [],
    ...over,
  };
}

const SPAN = { first: '2026-06-10T00:00:00+00:00', last: '2026-06-10T10:00:00+00:00' };

describe('AnalysisPanel — 검증 리듬 (§3b)', () => {
  test('run을 시간 기준 pct 점으로 렌더한다 (02:30/10h → 25%)', () => {
    render(
      <AnalysisPanel
        metrics={m}
        verificationRuns={[
          mkRun({ verification_run_id: 'vr1', started_at: '2026-06-10T02:30:00+00:00', status: 'failed', trigger_event_id: 'ev_f' }),
          mkRun({ verification_run_id: 'vr2', started_at: '2026-06-10T05:00:00+00:00', status: 'passed', trigger_event_id: 'ev_p' }),
        ]}
        sessionSpan={SPAN}
      />,
    );
    const dots = document.querySelectorAll('[data-dot]');
    expect(dots).toHaveLength(2);
    expect((dots[0] as HTMLElement).style.left).toBe('25%');
    expect((dots[1] as HTMLElement).style.left).toBe('50%');
  });

  test('점 클릭 → onSelectEvent(trigger_event_id)', () => {
    const onSelect = vi.fn();
    render(
      <AnalysisPanel
        metrics={m}
        verificationRuns={[mkRun({ trigger_event_id: 'ev_jump' })]}
        sessionSpan={SPAN}
        onSelectEvent={onSelect}
      />,
    );
    fireEvent.click(document.querySelector('button[data-dot]')!);
    expect(onSelect).toHaveBeenCalledWith('ev_jump');
  });

  test('run 0건이면 리듬 값 자리에 —', () => {
    render(<AnalysisPanel metrics={m} verificationRuns={[]} sessionSpan={SPAN} />);
    expect(screen.getByTestId('rhythm-empty')).toHaveTextContent('—');
  });
});

describe('AnalysisPanel — 변경 커버리지 (§3c)', () => {
  test('coverage 바와 커버 %·미커버 수를 렌더한다', () => {
    render(<AnalysisPanel metrics={m} coverage={{ covered: 3, total: 4 }} />);
    const bar = document.querySelector('[data-coverage-bar]');
    expect(bar).not.toBeNull();
    // 주의: /75%/ 단독 매칭은 기존 검증률 75% 행과 중복돼 getByText가 throw한다.
    expect(screen.getByText(/커버 75% · 미커버 1|covered 75% · 1 uncovered/)).toBeInTheDocument();
  });

  test('hunk 0건이면 커버리지 값 자리에 — (0%로 위장 금지)', () => {
    render(<AnalysisPanel metrics={m} coverage={{ covered: 0, total: 0 }} />);
    expect(screen.getByTestId('coverage-empty')).toHaveTextContent('—');
    expect(document.querySelector('[data-coverage-bar]')).toBeNull();
  });

  test('coverage 미전달(로딩/미지원)에도 — 표기', () => {
    render(<AnalysisPanel metrics={m} />);
    expect(screen.getByTestId('coverage-empty')).toHaveTextContent('—');
  });
});
