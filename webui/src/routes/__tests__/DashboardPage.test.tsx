import React from 'react';
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor, cleanup } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import '@testing-library/jest-dom/vitest';
import DashboardPage from '../DashboardPage';
import { I18nProvider } from '../../i18n';

function withRouter(node: React.ReactNode) {
  return (
    <I18nProvider initialLocale="en">
      <MemoryRouter initialEntries={['/dashboard?project=all']}>{node}</MemoryRouter>
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
      tool_call_total: 5,
      tool_failure_count: 1,
      verification_total: passed,
      verification_passed: passed,
      verification_failed: 0,
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

// fetch 라우팅: /v1/sessions(프로젝트 선택지)와 /v1/metrics(series)를 분기.
function mockFetch(series: unknown) {
  (fetch as unknown as ReturnType<typeof vi.fn>).mockImplementation((input: RequestInfo) => {
    const url = String(input);
    if (url.startsWith('/v1/metrics')) return Promise.resolve(envelope(series));
    if (url.startsWith('/v1/sessions')) return Promise.resolve(envelope([]));
    return Promise.reject(new Error(`unexpected fetch: ${url}`));
  });
}

describe('DashboardPage', () => {
  beforeEach(() => {
    vi.stubGlobal('fetch', vi.fn());
  });
  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
  });

  it('renders empty state with ingest hint', async () => {
    mockFetch({ sessions: [], session_count: 0, matched_count: 0 });
    render(withRouter(<DashboardPage />));
    await waitFor(() => expect(screen.getByText(/no sessions in this window/i)).toBeInTheDocument());
    expect(screen.getByText(/wimcc ingest --all/)).toBeInTheDocument();
  });

  it('renders cohort segments with direct labels and charts', async () => {
    mockFetch({
      sessions: [
        seriesRow('b2', '2026-07-02T00:00:00+00:00', ['claude-fable-5'], ['2.1.200'], 3),
        seriesRow('a1', '2026-07-01T00:00:00+00:00', ['claude-opus-4-7'], ['2.1.198'], 1),
      ],
      session_count: 2,
      matched_count: 2,
    });
    render(withRouter(<DashboardPage />));
    await waitFor(() => expect(screen.getByText('claude-opus-4-7')).toBeInTheDocument());
    expect(screen.getByText('claude-fable-5')).toBeInTheDocument();
    expect(screen.getByText('2.1.198')).toBeInTheDocument();
    expect(screen.getByText('2.1.200')).toBeInTheDocument();
    // outcome 차트와 프로세스 strip이 세션당 클릭 타깃(딥링크)을 만든다:
    // 블록 7개(outcome + strip 6) × 세션 2 = 14.
    const cols = screen.getAllByRole('button', { name: /open session/i });
    expect(cols.length).toBe(14);
    expect(screen.getByText('2 sessions')).toBeInTheDocument();
  });

  it('surfaces the limit truncation (no silent cap)', async () => {
    mockFetch({
      sessions: [seriesRow('a1', '2026-07-01T00:00:00+00:00', ['m'], ['1'], 1)],
      session_count: 1,
      matched_count: 320,
    });
    render(withRouter(<DashboardPage />));
    await waitFor(() =>
      expect(screen.getByText(/showing the latest 1 of 320 sessions/i)).toBeInTheDocument(),
    );
  });

  it('renders the data table with one row per session', async () => {
    mockFetch({
      sessions: [
        seriesRow('b2', '2026-07-02T00:00:00+00:00', ['m'], ['1'], 2),
        seriesRow('a1', '2026-07-01T00:00:00+00:00', ['m'], ['1'], 1),
      ],
      session_count: 2,
      matched_count: 2,
    });
    render(withRouter(<DashboardPage />));
    await waitFor(() => expect(screen.getByText(/data table/i)).toBeInTheDocument());
    const rows = screen.getAllByRole('row');
    // 헤더 1 + 세션 2.
    expect(rows.length).toBe(3);
  });

  it('shows the error state on API failure', async () => {
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
