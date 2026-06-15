import React from 'react';
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor, fireEvent, within, cleanup } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import '@testing-library/jest-dom/vitest';
import SessionListPage from '../SessionListPage';

function withRouter(node: React.ReactNode) {
  return <MemoryRouter>{node}</MemoryRouter>;
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
    expect(within(row).getByText('—')).toBeInTheDocument();
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
});
