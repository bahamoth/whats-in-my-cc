/** 코호트 경계 섹션 — 유의 경계 노출(초과율 문구)·차원 수동 탐색·슬로프 렌더. */
import { render, screen, cleanup } from '@testing-library/react';
import { describe, expect, it, vi, afterEach, beforeAll } from 'vitest';
import { CohortBoundaries } from '../CohortBoundaries';
import { rankCohorts } from '../../../lib/dashDerive';
import { I18nProvider } from '../../../i18n';
import type { SessionSeriesRowDto } from '../../../api/types';

vi.mock('../EChart', () => ({
  EChart: () => <div data-testid="echart" />,
}));
beforeAll(() => {
  vi.stubGlobal(
    'ResizeObserver',
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  );
});
afterEach(cleanup);

function row(id: string, date: string, models: string[], cost: number): SessionSeriesRowDto {
  return {
    session_id: id,
    first_observed_at: `2026-${date}T04:00:00+00:00`,
    last_observed_at: `2026-${date}T08:00:00+00:00`,
    event_count: 1000,
    metrics: {
      session_id: id,
      tool_call_total: 100,
      tool_failure_count: 2,
      verification_total: 12,
      verification_passed: 10,
      verification_failed: 2,
      verification_unknown: 0,
      verification_not_executed: 0,
      context_bloat_count: 0,
      tool_user_rejected: 0,
      tool_policy_denied: 0,
      tool_cancelled: 0,
      tool_backgrounded: 0,
      turn_duration_ms_total: 0,
      turn_duration_count: 0,
      api_error_count: 0,
      api_rate_limit_count: 0,
      input_tokens: 100_000,
      output_tokens: 500_000,
      cache_read_input_tokens: 9_500_000,
      cache_creation_input_tokens: 400_000,
      estimated_cost_usd: cost,
      compact_boundary_count: 0,
      tool_result_truncated_count: 0,
      user_interruption_count: 0,
      detector_firing: {},
      llm_request_p50: {
        ttft_ms: { p50: null, n: 0 },
        duration_ms: { p50: null, n: 0 },
        output_tokens: { p50: null, n: 0 },
        cost_usd: { p50: null, n: 0 },
      },
    },
    fingerprint: {
      session_id: id,
      models,
      cc_versions: ['2.1.200'],
      git_branches: [],
      cwds: [],
      entrypoints: [],
    },
  };
}

const jump = [
  ...Array.from({ length: 6 }, (_, i) => row(`a${i}`, `06-0${i + 1}`, ['claude-opus-4-8'], 10)),
  ...Array.from({ length: 6 }, (_, i) => row(`b${i}`, `06-1${i + 1}`, ['claude-fable-5'], 30)),
];

function mount(rows: SessionSeriesRowDto[]) {
  return render(
    <I18nProvider initialLocale="en">
      <CohortBoundaries ranked={rankCohorts(rows)} />
    </I18nProvider>,
  );
}

describe('CohortBoundaries', () => {
  it('유의 경계를 초과율 문구와 함께 노출하고 슬로프 4장을 렌더', () => {
    mount(jump);
    expect(screen.getByText(/cohort boundaries/i)).toBeInTheDocument();
    expect(screen.getAllByText(/top \d+% vs random splits/).length).toBeGreaterThan(0);
    expect(screen.getAllByText(/Opus 4.8 → Fable 5/).length).toBeGreaterThan(0);
    expect(screen.getAllByTestId('echart')).toHaveLength(4);
    expect(screen.getByText(/before 6 · after 6 sessions/i)).toBeInTheDocument();
  });
  it('차원 레일: 카운트 배지, 경계 0 차원은 비활성', () => {
    mount(jump);
    const model = screen.getByRole('button', { name: /model\s*1/ });
    expect(model).toBeEnabled();
    // 맥락 차원(branch·cwd)은 레일에서 제외 — 개입 차원만 남는다(4차 개정).
    expect(screen.queryByRole('button', { name: /branch/ })).toBeNull();
    const plugins = screen.getByRole('button', { name: /plugins\s*0/ });
    expect(plugins).toBeDisabled();
  });
  it('선택 행은 data-selected=true + 라디오 도트', () => {
    mount(jump);
    const rows = screen.getAllByText(/top \d+% vs random splits/).map((el) => el.closest('button')!);
    expect(rows[0].dataset.selected).toBe('true');
    expect(rows[0].textContent).toContain('●');
  });
  it('경계가 하나도 없으면 섹션 미렌더', () => {
    const flat = Array.from({ length: 6 }, (_, i) => row(`a${i}`, `06-0${i + 1}`, ['claude-opus-4-8'], 10));
    const { container } = mount(flat);
    expect(container.textContent).toBe('');
  });
});
