/**
 * 대시보드 전면 개편(2026-07-04) 골격 — 개요/검증 탭, 문자 헤드라인,
 * 이전 창 delta, 오류 상태를 잠근다. 차트 모듈 자체는 각 컴포넌트
 * 테스트가 SSOT (ECharts는 jsdom에서 mock).
 */
import React from 'react';
import { describe, expect, it, vi, beforeEach, afterEach, beforeAll } from 'vitest';
import { render, screen, waitFor, cleanup } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router-dom';
import '@testing-library/jest-dom/vitest';
import DashboardPage from '../DashboardPage';
import { I18nProvider } from '../../i18n';

vi.mock('../../components/dash/EChart', () => ({
  EChart: ({ height }: { height: number }) => <div data-testid="echart" style={{ height }} />,
}));

function withRouter(node: React.ReactNode, entry = '/dashboard?project=all') {
  return (
    <I18nProvider initialLocale="en">
      <MemoryRouter initialEntries={[entry]}>{node}</MemoryRouter>
    </I18nProvider>
  );
}

function envelope(data: unknown) {
  return new Response(JSON.stringify({ meta: { generated_at: 'n' }, data }), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  });
}

function seriesRow(id: string, first: string, models: string[], cc: string[], passed: number) {
  return {
    session_id: id,
    first_observed_at: first,
    last_observed_at: first,
    event_count: 10,
    metrics: {
      session_id: id,
      tool_call_total: 100,
      tool_failure_count: 2,
      verification_total: passed + 1,
      verification_passed: passed,
      verification_failed: 1,
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
      estimated_cost_usd: 50,
      compact_boundary_count: 0,
      tool_result_truncated_count: 0,
      user_interruption_count: 0,
      detector_firing: {},
    },
    fingerprint: {
      session_id: id,
      models,
      cc_versions: cc,
      git_branches: [],
      cwds: [],
      entrypoints: [],
    },
  };
}

const VSUM = {
  total: 6,
  measured: 4,
  passed: 2,
  failed: 2,
  unknown: 1,
  unknown_piped: 1,
  unknown_other: 0,
  not_executed: 1,
  by_kind: [{ kind: 'test', passed: 2, failed: 2, unknown: 0, not_executed: 0 }],
  failures: { recovered: 1, abandoned: 1 },
  rhythm: [],
  coverage: { covered: 3, total: 4, by_session: [] },
};

/** fetch 라우팅 — /v1/metrics는 `to=` 유무로 현재/이전 창을 구분한다. */
function mockFetch(cur: unknown, prev: unknown = { sessions: [], session_count: 0, matched_count: 0 }) {
  (fetch as unknown as ReturnType<typeof vi.fn>).mockImplementation((input: RequestInfo) => {
    const url = String(input);
    if (url.startsWith('/v1/metrics'))
      return Promise.resolve(envelope(url.includes('to=') ? prev : cur));
    if (url.startsWith('/v1/verification/summary')) return Promise.resolve(envelope(VSUM));
    if (url.startsWith('/v1/sessions')) return Promise.resolve(envelope([]));
    return Promise.reject(new Error(`unexpected fetch: ${url}`));
  });
}

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

describe('DashboardPage', () => {
  beforeEach(() => {
    vi.stubGlobal('fetch', vi.fn());
  });
  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
  });

  it('빈 창은 ingest 힌트와 함께 empty 상태', async () => {
    mockFetch({ sessions: [], session_count: 0, matched_count: 0 });
    render(withRouter(<DashboardPage />));
    await waitFor(() => expect(screen.getByText(/no sessions in this window/i)).toBeInTheDocument());
    expect(screen.getByText(/wimcc ingest --all/)).toBeInTheDocument();
  });

  it('문자 헤드라인 stat 5개 + 이전 창 delta 칩 렌더 (30d 창)', async () => {
    mockFetch(
      {
        sessions: [seriesRow('b2', '2026-07-02T00:00:00+00:00', ['claude-fable-5'], ['2.1.200'], 9)],
        session_count: 1,
        matched_count: 1,
      },
      {
        sessions: [seriesRow('a1', '2026-06-05T00:00:00+00:00', ['claude-opus-4-8'], ['2.1.198'], 4)],
        session_count: 1,
        matched_count: 1,
      },
    );
    render(withRouter(<DashboardPage />, '/dashboard?project=all&w=30d'));
    await waitFor(() => expect(screen.getByText(/verification pass/i)).toBeInTheDocument());
    expect(screen.getAllByText(/estimated cost/i).length).toBeGreaterThan(0);
    expect(screen.getByText(/blended unit rate/i)).toBeInTheDocument();
    expect(screen.getByText(/cache hit/i)).toBeInTheDocument();
    expect(screen.getByText(/tool failure rate/i)).toBeInTheDocument();
    // cur pass 90%, prev 80% → ▲ 10%p delta 칩
    await waitFor(() => expect(screen.getByText(/▲ 10%p/)).toBeInTheDocument());
    // 관측된 변화 줄
    expect(screen.getByText(/observed changes/i)).toBeInTheDocument();
  });

  it("전체 창(all)은 delta 없이 '비교 없음' fnote", async () => {
    mockFetch({
      sessions: [seriesRow('b2', '2026-07-02T00:00:00+00:00', ['claude-fable-5'], ['2.1.200'], 9)],
      session_count: 1,
      matched_count: 1,
    });
    render(withRouter(<DashboardPage />));
    await waitFor(() => expect(screen.getByText(/verification pass/i)).toBeInTheDocument());
    expect(screen.getAllByText(/no comparison/i).length).toBeGreaterThan(0);
    expect(screen.queryByText(/▲/)).not.toBeInTheDocument();
  });

  it('검증 탭 전환 시 summary를 fetch해 렌더', async () => {
    mockFetch({
      sessions: [seriesRow('b2', '2026-07-02T00:00:00+00:00', ['claude-fable-5'], ['2.1.200'], 9)],
      session_count: 1,
      matched_count: 1,
    });
    render(withRouter(<DashboardPage />));
    await waitFor(() => expect(screen.getByText(/verification pass/i)).toBeInTheDocument());
    const tab = await screen.findByRole('tab', { name: /verification$/i });
    await userEvent.click(tab);
    await waitFor(() =>
      expect(
        (fetch as unknown as ReturnType<typeof vi.fn>).mock.calls.some((c) =>
          String(c[0]).startsWith('/v1/verification/summary'),
        ),
      ).toBe(true),
    );
  });

  it('API 실패 시 role=alert', async () => {
    (fetch as unknown as ReturnType<typeof vi.fn>).mockImplementation((input: RequestInfo) => {
      const url = String(input);
      if (url.startsWith('/v1/metrics'))
        return Promise.resolve(
          new Response(JSON.stringify({ detail: 'boom' }), {
            status: 500,
            headers: { 'content-type': 'application/json' },
          }),
        );
      return Promise.resolve(envelope([]));
    });
    render(withRouter(<DashboardPage />));
    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent(/failed to load/i));
  });
});
