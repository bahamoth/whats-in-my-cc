import React from 'react';
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor, fireEvent, within, cleanup } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import '@testing-library/jest-dom/vitest';
import SessionListPage from '../SessionListPage';
import { I18nProvider } from '../../i18n';

function withRouter(node: React.ReactNode) {
  return (
    <I18nProvider initialLocale="en">
      <MemoryRouter>{node}</MemoryRouter>
    </I18nProvider>
  );
}

describe('SessionListPage', () => {
  beforeEach(() => { vi.stubGlobal('fetch', vi.fn()); });
  afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

  function envelope(data: unknown) {
    return new Response(JSON.stringify({ meta: { generated_at: 'n' }, data }), {
      status: 200, headers: { 'content-type': 'application/json' },
    });
  }

  it('renders empty state with CLI hint', async () => {
    (fetch as unknown as ReturnType<typeof vi.fn>).mockResolvedValueOnce(envelope([]));
    render(withRouter(<SessionListPage />));
    await waitFor(() => expect(screen.getByText(/no sessions yet/i)).toBeInTheDocument());
    expect(screen.getByText(/wimcc ingest --all/)).toBeInTheDocument();
  });

  it('renders rows sorted by last_observed_at desc', async () => {
    (fetch as unknown as ReturnType<typeof vi.fn>).mockResolvedValueOnce(envelope([
      { session_id: 'older', first_observed_at: '2026-05-19T08:00:00Z', last_observed_at: '2026-05-19T09:00:00Z', event_count: 5, source_uris: [] },
      { session_id: 'newer', first_observed_at: '2026-05-19T10:00:00Z', last_observed_at: '2026-05-19T11:00:00Z', event_count: 7, source_uris: [] },
    ]));
    render(withRouter(<SessionListPage />));
    const rows = await screen.findAllByRole('row');
    // [header, newer, older]
    expect(rows[1]).toHaveTextContent('newer');
    expect(rows[2]).toHaveTextContent('older');
  });

  it('renders error state with retry', async () => {
    (fetch as unknown as ReturnType<typeof vi.fn>).mockResolvedValueOnce(
      new Response('{"detail":"db gone"}', { status: 500 })
    );
    render(withRouter(<SessionListPage />));
    await waitFor(() => expect(screen.getByText(/db gone/)).toBeInTheDocument());
    expect(screen.getByRole('button', { name: /retry/i })).toBeInTheDocument();
  });

  // ---- slice-7 SessionList source mix (retained behaviour) ----

  it('renders source-mix tags from by_kind (transcript + hook)', async () => {
    (fetch as unknown as ReturnType<typeof vi.fn>).mockResolvedValueOnce(envelope([
      {
        session_id: 'mixed',
        first_observed_at: '2026-05-21T00:00:00Z',
        last_observed_at: '2026-05-21T01:00:00Z',
        event_count: 100,
        source_uris: [],
        by_kind: {
          user_message: 10,
          assistant_message: 10,
          tool_call: 30,
          tool_result: 30,
          hook_event: 20,
        },
      },
    ]));
    render(withRouter(<SessionListPage />));
    await screen.findByText('mixed');
    // transcript bucket = user_message + assistant_message + tool_call + tool_result = 80
    expect(screen.getByText(/txn\s+80/)).toBeInTheDocument();
    // hook bucket = 20
    expect(screen.getByText(/hook\s+20/)).toBeInTheDocument();
  });

  it('marks OTel-only sessions distinctly (no transcript, no hook, only otel)', async () => {
    (fetch as unknown as ReturnType<typeof vi.fn>).mockResolvedValueOnce(envelope([
      {
        session_id: 'otel-orphan',
        first_observed_at: '2026-05-21T00:00:00Z',
        last_observed_at: '2026-05-21T00:01:00Z',
        event_count: 10,
        source_uris: [],
        by_kind: { log_record: 9, metric_sample: 1 },
      },
    ]));
    render(withRouter(<SessionListPage />));
    const link = await screen.findByText('otel-orphan');
    const row = link.closest('tr')!;
    // visual cue class — exact name comes from CSS modules so we match a substring
    expect(row.className).toMatch(/otelOnly/);
    expect(within(row).getByText(/otel\s+10/)).toBeInTheDocument();
  });

  it('renders dash placeholder when by_kind is absent (older server)', async () => {
    (fetch as unknown as ReturnType<typeof vi.fn>).mockResolvedValueOnce(envelope([
      {
        session_id: 'legacy',
        first_observed_at: '2026-05-21T00:00:00Z',
        last_observed_at: '2026-05-21T01:00:00Z',
        event_count: 5,
        source_uris: [],
        // no by_kind field
      },
    ]));
    render(withRouter(<SessionListPage />));
    const link = await screen.findByText('legacy');
    const row = link.closest('tr')!;
    expect(within(row).getAllByText('—').length).toBeGreaterThan(0);
  });

  function headerByLabel(label: string): HTMLElement {
    const headers = Array.from(document.querySelectorAll('thead th')) as HTMLElement[];
    const match = headers.find((h) => h.textContent?.toLowerCase().includes(label.toLowerCase()));
    if (!match) throw new Error(`no header containing "${label}"`);
    return match;
  }

  it('shows ▼ on the last-seen header by default and renders sort hint', async () => {
    (fetch as unknown as ReturnType<typeof vi.fn>).mockResolvedValueOnce(envelope([
      { session_id: 's1', first_observed_at: 'a', last_observed_at: 'b', event_count: 1, source_uris: [] },
    ]));
    render(withRouter(<SessionListPage />));
    await screen.findByText('s1');
    expect(headerByLabel('last seen').textContent).toMatch(/▼/);
    expect(screen.getByText(/sorted by/i)).toBeInTheDocument();
  });

  it('clicking events header re-sorts by events desc', async () => {
    (fetch as unknown as ReturnType<typeof vi.fn>).mockResolvedValueOnce(envelope([
      { session_id: 'small', first_observed_at: 'a', last_observed_at: 'z', event_count: 5, source_uris: [] },
      { session_id: 'large', first_observed_at: 'b', last_observed_at: 'y', event_count: 999, source_uris: [] },
      { session_id: 'mid',   first_observed_at: 'c', last_observed_at: 'x', event_count: 50, source_uris: [] },
    ]));
    render(withRouter(<SessionListPage />));
    await screen.findByText('small');
    fireEvent.click(headerByLabel('events'));
    const rows = screen.getAllByRole('row');
    // [header, large(999), mid(50), small(5)]
    expect(rows[1]).toHaveTextContent('large');
    expect(rows[2]).toHaveTextContent('mid');
    expect(rows[3]).toHaveTextContent('small');
  });

  it('slice-8: LIVE badge OFF by default even when last_observed_at is recent', async () => {
    const now = new Date();
    const recent = new Date(now.getTime() - 5_000).toISOString();
    (fetch as unknown as ReturnType<typeof vi.fn>).mockResolvedValueOnce(envelope([
      { session_id: 'aaa', first_observed_at: '2026-05-21T00:00:00Z', last_observed_at: recent, event_count: 1, source_uris: [] },
    ]));
    render(withRouter(<SessionListPage />));
    const row = (await screen.findByText('aaa')).closest('tr')!;
    expect(within(row).queryByTestId('live-badge')).toBeNull();
  });

  it('clicking the active header again flips the sort direction', async () => {
    (fetch as unknown as ReturnType<typeof vi.fn>).mockResolvedValueOnce(envelope([
      { session_id: 'aaa', first_observed_at: 'x', last_observed_at: '2026-05-21T01:00:00Z', event_count: 1, source_uris: [] },
      { session_id: 'bbb', first_observed_at: 'x', last_observed_at: '2026-05-21T02:00:00Z', event_count: 1, source_uris: [] },
    ]));
    render(withRouter(<SessionListPage />));
    await screen.findByText('aaa');
    const lastHeader = headerByLabel('last seen');
    // default desc: bbb on top
    let rows = screen.getAllByRole('row');
    expect(rows[1]).toHaveTextContent('bbb');
    // click → asc: aaa on top
    fireEvent.click(lastHeader);
    rows = screen.getAllByRole('row');
    expect(rows[1]).toHaveTextContent('aaa');
    expect(headerByLabel('last seen').textContent).toMatch(/▲/);
  });

  // ---- S6 (UX 재설계) — slug · project · model · preview · search ----

  function richRow(over: Partial<Record<string, unknown>> = {}) {
    return {
      session_id: '4e3cdf37-1a3d-4d0e-af6b-6fa322574d6e',
      first_observed_at: '2026-06-15T00:00:00Z',
      last_observed_at: '2026-06-15T01:00:00Z',
      event_count: 24749,
      source_uris: [],
      by_kind: { user_message: 5 },
      slug: 'resilient-jingling-lark',
      project: '/Users/bahamoth/projects/whats-in-my-cc',
      model: 'claude-opus-4-8',
      first_user_message_preview: '병렬 배치 묶기 UI 개선 — 서브에이전트 그룹핑 누락 조사',
      ...over,
    };
  }

  it('shows the slug as the primary label, with the UUID reachable via the link', async () => {
    (fetch as unknown as ReturnType<typeof vi.fn>).mockResolvedValueOnce(envelope([richRow()]));
    render(withRouter(<SessionListPage />));
    const slug = await screen.findByText('resilient-jingling-lark');
    const link = slug.closest('a')!;
    expect(link).toHaveAttribute('href', '/sessions/4e3cdf37-1a3d-4d0e-af6b-6fa322574d6e');
  });

  it('falls back to the UUID as the label when no slug is present', async () => {
    (fetch as unknown as ReturnType<typeof vi.fn>).mockResolvedValueOnce(
      envelope([richRow({ slug: undefined })]),
    );
    render(withRouter(<SessionListPage />));
    const link = await screen.findByText('4e3cdf37-1a3d-4d0e-af6b-6fa322574d6e');
    expect(link.closest('a')).toHaveAttribute(
      'href',
      '/sessions/4e3cdf37-1a3d-4d0e-af6b-6fa322574d6e',
    );
  });

  it('renders a project pill (basename) and a humanised model tag', async () => {
    (fetch as unknown as ReturnType<typeof vi.fn>).mockResolvedValueOnce(envelope([richRow()]));
    render(withRouter(<SessionListPage />));
    await screen.findByText('resilient-jingling-lark');
    expect(screen.getByText('whats-in-my-cc')).toBeInTheDocument();
    expect(screen.getByText('Opus 4.8')).toBeInTheDocument();
  });

  it('renders the first-user-message preview', async () => {
    (fetch as unknown as ReturnType<typeof vi.fn>).mockResolvedValueOnce(envelope([richRow()]));
    render(withRouter(<SessionListPage />));
    expect(
      await screen.findByText(/병렬 배치 묶기 UI 개선/),
    ).toBeInTheDocument();
  });

  it('focuses the search box when "/" is pressed (S10 keyboard)', async () => {
    (fetch as unknown as ReturnType<typeof vi.fn>).mockResolvedValueOnce(envelope([richRow()]));
    render(withRouter(<SessionListPage />));
    await screen.findByText('resilient-jingling-lark');
    const search = screen.getByPlaceholderText(/검색|search/i);
    expect(search).not.toHaveFocus();
    fireEvent.keyDown(window, { key: '/' });
    expect(search).toHaveFocus();
  });

  it('filters rows by slug / project substring via the search box', async () => {
    (fetch as unknown as ReturnType<typeof vi.fn>).mockResolvedValueOnce(envelope([
      richRow(),
      richRow({
        session_id: 'bbb-2222',
        slug: 'quiet-meadow-flux',
        project: '/Users/bahamoth/projects/cc-dev-tools',
        model: 'claude-sonnet-4-6',
        first_user_message_preview: '플러그인 hook frontmatter 사고',
      }),
    ]));
    render(withRouter(<SessionListPage />));
    await screen.findByText('resilient-jingling-lark');
    const search = screen.getByPlaceholderText(/검색|search/i);
    fireEvent.change(search, { target: { value: 'meadow' } });
    expect(screen.queryByText('resilient-jingling-lark')).toBeNull();
    expect(screen.getByText('quiet-meadow-flux')).toBeInTheDocument();
  });

  // ---- 2026-07-04 리스트 지표 컬럼(추가형) ----

  const METRICS_ROW = {
    session_id: 'mixed',
    first_observed_at: '2026-05-21T00:00:00Z',
    last_observed_at: '2026-05-21T01:00:00Z',
    event_count: 100,
    metrics: {
      session_id: 'mixed',
      tool_call_total: 50,
      tool_failure_count: 3,
      verification_total: 12,
      verification_passed: 8,
      verification_failed: 2,
      verification_unknown: 2,
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
      input_tokens: 100_000,
      output_tokens: 500_000,
      cache_read_input_tokens: 9_400_000,
      cache_creation_input_tokens: 400_000,
      estimated_cost_usd: 42.5,
      compact_boundary_count: 0,
      tool_result_truncated_count: 0,
      user_interruption_count: 0,
      detector_firing: {},
    },
    fingerprint: { session_id: 'mixed', models: [], cc_versions: [], git_branches: [], cwds: [], entrypoints: [] },
  };

  function routeFetch(sessions: unknown[], series: unknown[] | null) {
    (fetch as unknown as ReturnType<typeof vi.fn>).mockImplementation((input: RequestInfo) => {
      const url = String(input);
      if (url.startsWith('/v1/metrics')) {
        if (series === null) return Promise.resolve(new Response('{"detail":"x"}', { status: 500 }));
        return Promise.resolve(envelope({ sessions: series, session_count: series.length, matched_count: series.length }));
      }
      if (url.startsWith('/v1/sessions')) return Promise.resolve(envelope(sessions));
      return Promise.reject(new Error(`unexpected: ${url}`));
    });
  }

  it('metrics join — 검증·신호·비용·단가·적중 컬럼을 렌더한다', async () => {
    routeFetch(
      [{ session_id: 'mixed', first_observed_at: '2026-05-21T00:00:00Z', last_observed_at: '2026-05-21T01:00:00Z', event_count: 100, source_uris: [] }],
      [METRICS_ROW],
    );
    render(withRouter(<SessionListPage />));
    await screen.findByText('mixed');
    await waitFor(() =>
      expect(
        screen.getByText((_, el) => el?.tagName === 'TD' && el.textContent?.startsWith('8/12') === true),
      ).toBeInTheDocument(),
    );
    expect(screen.getByText('$42.5')).toBeInTheDocument(); // 비용
    expect(screen.getByText('$42.5/1M')).toBeInTheDocument(); // 단가 (billed 1M)
    expect(screen.getByText('94.9%')).toBeInTheDocument(); // 적중
    expect(screen.getByText('4')).toBeInTheDocument(); // 신호 3+1
  });

  it('metrics fetch 실패 시 기존 컬럼은 정상, 지표는 —', async () => {
    routeFetch(
      [{ session_id: 'alone', first_observed_at: '2026-05-21T00:00:00Z', last_observed_at: '2026-05-21T01:00:00Z', event_count: 7, source_uris: [] }],
      null,
    );
    render(withRouter(<SessionListPage />));
    await screen.findByText('alone');
    expect(screen.getAllByText('—').length).toBeGreaterThan(0);
  });

  it('cost 헤더 클릭 → 비용 내림차순 정렬', async () => {
    const mk = (sid: string, cost: number) => ({
      ...METRICS_ROW,
      session_id: sid,
      metrics: { ...METRICS_ROW.metrics, session_id: sid, estimated_cost_usd: cost },
      fingerprint: { ...METRICS_ROW.fingerprint, session_id: sid },
    });
    routeFetch(
      [
        { session_id: 'cheap', first_observed_at: '2026-05-21T00:00:00Z', last_observed_at: '2026-05-21T02:00:00Z', event_count: 1, source_uris: [] },
        { session_id: 'pricey', first_observed_at: '2026-05-21T00:00:00Z', last_observed_at: '2026-05-21T01:00:00Z', event_count: 1, source_uris: [] },
      ],
      [mk('cheap', 1), mk('pricey', 900)],
    );
    render(withRouter(<SessionListPage />));
    await screen.findByText('cheap');
    fireEvent.click(screen.getByText(/^cost/));
    await waitFor(() => {
      const rows = screen.getAllByRole('row');
      expect(rows[1]).toHaveTextContent('pricey');
      expect(rows[2]).toHaveTextContent('cheap');
    });
  });

});
