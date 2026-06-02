import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { act, render, screen, waitFor, fireEvent, cleanup } from '@testing-library/react';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { QueryClientProvider } from '@tanstack/react-query';
import '@testing-library/jest-dom/vitest';
import SessionDetailPage from '../SessionDetailPage';
import { MockEventSource } from '../../test/MockEventSource';
import { createQueryClient } from '../../lib/queryClient';

function rendered(sessionId: string) {
  const qc = createQueryClient();
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={[`/sessions/${sessionId}`]}>
        <Routes>
          <Route path="/sessions/:sessionId" element={<SessionDetailPage />} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

// Slice-9 — page fans out three independent fetches on mount (summary,
// graph, windowed events). Order between them is unspecified by React's
// useEffect scheduler, so tests dispatch by URL rather than by call order.
type Routes = {
  detail?: Response;
  graph?: Response;
  events?: Response;
  raw?: Response;
  findings?: Response;
};

function setupFetch(routes: Routes) {
  const fn = vi.fn((input: RequestInfo | URL) => {
    const url = typeof input === 'string' ? input : input.toString();
    // Older-history page (`?before=`): return an empty page (as the server does
    // at the session start). This exercises the loadOlder fetch without the
    // mock re-returning already-loaded rows (which would duplicate keys).
    if (url.includes('/events?') && url.includes('before=')) {
      return Promise.resolve(env({ events: [], prev_cursor: null, next_cursor: null }));
    }
    if (url.includes('/events?') || url.endsWith('/events')) {
      // window endpoint
      const m = url.match(/\/v1\/sessions\/[^/]+\/events/);
      if (m && routes.events) return Promise.resolve(routes.events.clone());
    }
    if (url.includes('/graph')) {
      if (routes.graph) return Promise.resolve(routes.graph.clone());
    }
    if (url.match(/\/v1\/events\//) && url.endsWith('/raw')) {
      if (routes.raw) return Promise.resolve(routes.raw.clone());
    }
    if (url.includes('/findings')) {
      if (routes.findings) return Promise.resolve(routes.findings.clone());
    }
    if (url.match(/\/v1\/sessions\/[^/]+$/)) {
      if (routes.detail) return Promise.resolve(routes.detail.clone());
    }
    if (url.includes('/v1/stream')) {
      // SSE not involved in these tests
      return Promise.resolve(new Response('', { status: 200 }));
    }
    return Promise.resolve(new Response('{}', { status: 404 }));
  });
  vi.stubGlobal('fetch', fn);
  return fn;
}

const sessionDetail = {
  session_id: 's1',
  summary: {
    event_count: 2,
    by_kind: { user_message: 1, assistant_message: 1 },
    first_observed_at: '2026-05-19T10:00:00Z',
    last_observed_at: '2026-05-19T10:00:05Z',
  },
};

const graph = {
  nodes: [
    { node_id: 'n1', schema_version: '1.0', session_id: 's1', node_kind: 'user_message',
      started_at: '2026-05-19T10:00:00Z', ended_at: null, merge_keys: {},
      source_event_ids: ['ev1'], source_uris: [], payload: {} },
    { node_id: 'n2', schema_version: '1.0', session_id: 's1', node_kind: 'assistant_message',
      started_at: '2026-05-19T10:00:05Z', ended_at: null, merge_keys: {},
      source_event_ids: ['ev2'], source_uris: [], payload: {} },
  ],
  edges: [
    { edge_id: 'e1', schema_version: '1.0', session_id: 's1',
      from_node_id: 'n1', to_node_id: 'n2', edge_kind: 'message_reply',
      origin: 'deterministic', attributes: {} },
  ],
};

const eventsPayload = {
  events: [],
  prev_cursor: null,
  next_cursor: null,
};

// Real conversation rows whose event_ids match the graph nodes'
// source_event_ids (n1→ev1, n2→ev2). Used to exercise stream-card selection
// end-to-end (empty `eventsPayload` mounts zero StreamCards and therefore
// cannot prove the wiring).
const eventsWithRows = {
  events: [
    {
      event_id: 'ev1', raw_event_id: 'r1', session_id: 's1', event_uuid: null,
      parent_uuid: null, observed_at: '2026-05-19T10:00:00Z', actor: 'user',
      kind: 'user_message', subkind: null, tool_use_id: null, tool_name: null,
      turn_id: null, is_sidechain: false, is_meta: false,
      payload: { content: 'hello from user' },
    },
    {
      event_id: 'ev2', raw_event_id: 'r2', session_id: 's1', event_uuid: null,
      parent_uuid: null, observed_at: '2026-05-19T10:00:05Z', actor: 'assistant',
      kind: 'assistant_message', subkind: null, tool_use_id: null, tool_name: null,
      turn_id: null, is_sidechain: false, is_meta: false,
      payload: { text: 'hi from assistant' },
    },
  ],
  prev_cursor: null,
  next_cursor: null,
};

const raw = {
  schema_version: '1.0', event_id: 'ev1', session_id: 's1',
  source: { kind: 'claude_transcript', file_path: '/tmp/a.jsonl', line_no: 1, ingested_at: 'n' },
  record: { hello: 'world' }, record_type: 'user_message', redaction_state: 'none',
};

function env(data: unknown) {
  return new Response(JSON.stringify({ meta: { generated_at: 'n' }, data }), {
    status: 200, headers: { 'content-type': 'application/json' },
  });
}

describe('SessionDetailPage', () => {
  beforeEach(() => {
    // EventSource is referenced by useLiveStream; jsdom doesn't ship one.
    // A no-op shim is enough for these tests (we never dispatch envelopes).
    if (!(globalThis as { EventSource?: unknown }).EventSource) {
      (globalThis as Record<string, unknown>).EventSource = class FakeES {
        url: string;
        readyState = 0;
        onmessage: ((ev: MessageEvent) => void) | null = null;
        constructor(u: string) { this.url = u; }
        addEventListener() {}
        close() {}
      };
    }
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    cleanup();
  });

  it('renders meta strip + DetailPanel empty hint before node selection', async () => {
    setupFetch({ detail: env(sessionDetail), graph: env(graph), events: env(eventsPayload) });
    rendered('s1');
    await waitFor(() => expect(screen.getByText(/2 events/)).toBeInTheDocument());
    expect(screen.getByText(/select an event to inspect it/i)).toBeInTheDocument();
  });

  it('clicking a stream card shows the DetailPanel tablist', async () => {
    setupFetch({
      detail: env(sessionDetail),
      graph: env(graph),
      events: env(eventsWithRows),
      raw: env(raw),
    });
    const { container } = rendered('s1');
    const card = await waitFor(() => {
      const el = container.querySelector(
        '[data-event-id="ev1"] [data-testid="message-card"]',
      );
      if (!el) throw new Error('card not found');
      return el;
    });
    fireEvent.click(card);
    await waitFor(() => expect(screen.getByRole('tablist')).toBeInTheDocument());
    expect(screen.getByRole('tab', { name: /insight/i })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: /raw/i })).toBeInTheDocument();
    expect(screen.queryByRole('tab', { name: /^detail$/i })).toBeNull();
  });

  // The headline selection requirement: clicking a conversation stream card
  // selects it, and clicking another moves selection (reflected on the card's
  // data-selected). With the bottom timeline view removed, the stream is the
  // sole in-page selection source; empty-events tests mount zero cards and so
  // cannot prove this wiring — this is the only place it is proven end-to-end.
  it('syncs selection across stream cards', async () => {
    setupFetch({
      detail: env(sessionDetail),
      graph: env(graph),
      events: env(eventsWithRows),
      raw: env(raw),
    });
    const { container } = rendered('s1');

    // Real events mount MessageCards keyed by event id.
    await waitFor(() => {
      expect(
        container.querySelector('[data-event-id="ev1"] [data-testid="message-card"]'),
      ).not.toBeNull();
    });

    // Click the ev1 card → it becomes selected.
    const card1 = container.querySelector(
      '[data-event-id="ev1"] [data-testid="message-card"]',
    );
    expect(card1).not.toBeNull();
    fireEvent.click(card1!);
    await waitFor(() => {
      expect(
        container
          .querySelector('[data-event-id="ev1"] [data-testid="message-card"]')
          ?.getAttribute('data-selected'),
      ).toBe('true');
    });

    // Click the ev2 card → selection moves to ev2, off ev1.
    const card2 = container.querySelector(
      '[data-event-id="ev2"] [data-testid="message-card"]',
    );
    expect(card2).not.toBeNull();
    fireEvent.click(card2!);
    await waitFor(() => {
      expect(
        container
          .querySelector('[data-event-id="ev2"] [data-testid="message-card"]')
          ?.getAttribute('data-selected'),
      ).toBe('true');
    });
    expect(
      container
        .querySelector('[data-event-id="ev1"] [data-testid="message-card"]')
        ?.getAttribute('data-selected'),
    ).toBe('false');
  });

  it('shows 404 when session detail missing', async () => {
    setupFetch({
      detail: new Response('{"detail":"session nope not found"}', { status: 404 }),
      graph: new Response('{"detail":"no graph"}', { status: 404 }),
      events: env(eventsPayload),
    });
    rendered('nope');
    await waitFor(() => expect(screen.getByText(/session not found/i)).toBeInTheDocument());
  });

  it('renders the page (no timeline) when getGraph 404s but getSession succeeds', async () => {
    setupFetch({
      detail: env(sessionDetail),
      graph: new Response('{"detail":"no graph"}', { status: 404 }),
      events: env(eventsPayload),
    });
    const { container } = rendered('s1');
    await waitFor(() => expect(screen.getByText(/2 events/)).toBeInTheDocument());
    expect(screen.queryByText(/session not found/i)).not.toBeInTheDocument();
    // The stream slot still renders; the bottom timeline view has been removed,
    // so a graph 404 must not surface any timeline canvas.
    expect(container.querySelector('[data-slot="stream"]')).not.toBeNull();
    expect(container.querySelector('[data-slot="timeline"]')).toBeNull();
    expect(screen.queryByTestId('timeline-canvas')).toBeNull();
  });

  // Envelope-driven backfill. An envelope burst inside the debounce window
  // collapses to ONE forward events backfill (`?after=`) — not one per
  // envelope. SSE envelopes carry no payload, so we fetch the real events
  // instead of appending the empty envelope (the "live messages don't appear
  // until refresh" fix). The views are event-first now: the graph endpoint is
  // NEVER fetched (no node/graph dependency) and the summary is never re-hit.
  it('envelope burst triggers one events backfill and never fetches the graph', async () => {
    MockEventSource.install();
    const f = setupFetch({
      detail: env(sessionDetail),
      graph: env(graph),
      // Non-empty window so the backfill has a tail cursor to page `?after=`.
      events: env(eventsWithRows),
    });
    rendered('s1');
    await waitFor(() => expect(screen.getByText(/2 events/)).toBeInTheDocument());

    // Snapshot mount-time fetch counts.
    const callsOf = (matcher: (u: string) => boolean) =>
      f.mock.calls.filter((c) => matcher(String(c[0]))).length;
    const summaryAtMount = callsOf((u) => /\/v1\/sessions\/[^/]+$/.test(u));
    const eventsAtMount = callsOf((u) => u.includes('/events'));
    expect(summaryAtMount).toBe(1);
    expect(callsOf((u) => u.includes('/graph'))).toBe(0); // event-first: no graph
    expect(eventsAtMount).toBe(1);

    // Fire 5 envelopes in tight succession. They should collapse to exactly
    // one debounced forward backfill (`?after=`) — not one fetch per envelope.
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const es = MockEventSource.latest();
    expect(es).toBeDefined();
    act(() => {
      for (let i = 1; i <= 5; i++) {
        es!.emit(
          'message',
          JSON.stringify({
            schema_version: '1',
            session_id: 's1',
            event_id: `01J${String(i).padStart(23, '0')}`,
            kind: 'tool_call',
            source_type: 'transcript',
            observed_at: `2026-05-19T10:00:1${i}Z`,
          }),
        );
      }
    });

    // Advance just past the 800ms debounce window (and well before the
    // 10s summary interval).
    await act(async () => {
      vi.advanceTimersByTime(900);
    });
    vi.useRealTimers();

    // Exactly one forward backfill fetch (?after=), regardless of burst size.
    await waitFor(() => {
      expect(callsOf((u) => u.includes('/events') && u.includes('after='))).toBe(1);
    });
    // The graph endpoint is still never fetched, and summary is never re-hit.
    expect(callsOf((u) => u.includes('/graph'))).toBe(0);
    expect(callsOf((u) => /\/v1\/sessions\/[^/]+$/.test(u))).toBe(summaryAtMount);
    expect(callsOf((u) => u.includes('/events'))).toBe(eventsAtMount + 1);
  });

  // A `gap` frame (SSE broadcast lagged — the channel is shared across all
  // sessions, so it lags whenever ANY session is busy) must NOT wipe the
  // windowed buffer. Reloading on gap discarded every older page the reader had
  // scrolled back to load and snapped the view to the newest event. Instead it
  // catches the live tip up with a forward backfill (`?after=`); the initial
  // window fetch (`?limit=`) is NOT repeated (no reload).
  it('a gap catches the tip up with loadNewer, not a window-wiping reload', async () => {
    MockEventSource.install();
    const f = setupFetch({
      detail: env(sessionDetail),
      graph: env(graph),
      events: env({
        events: eventsWithRows.events, // non-empty → loadNewer has a tail cursor
        prev_cursor: '2026-05-19T10:00:00Z|01J',
        next_cursor: null,
      }),
    });
    rendered('s1');
    await waitFor(() => expect(screen.getByText(/2 events/)).toBeInTheDocument());

    const callsOf = (m: (u: string) => boolean) =>
      f.mock.calls.filter((c) => m(String(c[0]))).length;
    const initialWindowFetches = callsOf((u) => /\/events\?limit=/.test(u));
    expect(initialWindowFetches).toBe(1);

    // A gap frame arrives on the shared SSE stream.
    const es = MockEventSource.latest();
    expect(es).toBeDefined();
    act(() => es!.emit('gap', JSON.stringify({ dropped: 7 })));

    // It backfills the tip forward (?after=) ...
    await waitFor(() => {
      expect(callsOf((u) => u.includes('/events') && u.includes('after='))).toBe(1);
    });
    // ... and never re-runs the initial window fetch (i.e. never reloads/wipes).
    expect(callsOf((u) => /\/events\?limit=/.test(u))).toBe(initialWindowFetches);
  });

  // A `resync` frame (SSE reconnected with a cursor the backend can't backfill
  // — frequent when the shared broadcast is under load and the connection
  // drops) must ALSO not wipe the window. The older pages are REST-fetched and
  // authoritative; only the live tip needs catching up. So resync, like gap,
  // backfills forward (`?after=`) and never re-runs the initial window fetch.
  it('a resync catches the tip up with loadNewer, not a window-wiping reload', async () => {
    MockEventSource.install();
    const f = setupFetch({
      detail: env(sessionDetail),
      graph: env(graph),
      events: env({
        events: eventsWithRows.events,
        prev_cursor: '2026-05-19T10:00:00Z|01J',
        next_cursor: null,
      }),
    });
    rendered('s1');
    await waitFor(() => expect(screen.getByText(/2 events/)).toBeInTheDocument());

    const callsOf = (m: (u: string) => boolean) =>
      f.mock.calls.filter((c) => m(String(c[0]))).length;
    const initialWindowFetches = callsOf((u) => /\/events\?limit=/.test(u));
    expect(initialWindowFetches).toBe(1);

    const es = MockEventSource.latest();
    expect(es).toBeDefined();
    act(() => es!.emit('resync', JSON.stringify({ reason: 'cursor invalidated' })));

    await waitFor(() => {
      expect(callsOf((u) => u.includes('/events') && u.includes('after='))).toBe(1);
    });
    expect(callsOf((u) => /\/events\?limit=/.test(u))).toBe(initialWindowFetches);
  });

  // Windowing: older history is paged by the stream's own near-top scroll
  // (the IntersectionObserver sentinel was removed — it auto-loaded the whole
  // session). A genuine gesture that lands near the top fetches the next older
  // window (`?before=...`). prev_cursor is non-null so `canLoadOlder` is true.
  it('a near-top user scroll pages the next older window (?before=)', async () => {
    const f = setupFetch({
      detail: env(sessionDetail),
      graph: env(graph),
      events: env({
        events: eventsWithRows.events, // real rows → the stream scroller mounts
        prev_cursor: '2026-05-19T10:00:00Z|01J', // older history remains
        next_cursor: null,
      }),
    });
    const { container } = rendered('s1');
    // Wait until the window has loaded: rows mounted (so the scroll container
    // exists) and `oldest` is set (so canLoadOlder is true).
    const scroller = await waitFor(() => {
      const el = container.querySelector('[data-testid="conversation-stream"]');
      if (!el || !container.querySelector('[data-event-id="ev1"]')) {
        throw new Error('stream not mounted yet');
      }
      return el as HTMLElement;
    });
    // A genuine gesture (wheel) + an UPWARD near-top scroll pages the next
    // older window (start below the zone, then scroll up into it).
    fireEvent.wheel(scroller, { deltaY: -120 });
    scroller.scrollTop = 400;
    fireEvent.scroll(scroller);
    scroller.scrollTop = 0;
    fireEvent.scroll(scroller);
    await waitFor(() => {
      const calls = f.mock.calls.map((c) => String(c[0]));
      expect(calls.some((u) => u.includes('/events?before='))).toBe(true);
    });
  });
});

describe('R1 layout shell', () => {
  beforeEach(() => {
    if (!(globalThis as { EventSource?: unknown }).EventSource) {
      (globalThis as Record<string, unknown>).EventSource = class FakeES {
        url: string;
        readyState = 0;
        onmessage: ((ev: MessageEvent) => void) | null = null;
        constructor(u: string) { this.url = u; }
        addEventListener() {}
        close() {}
      };
    }
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    cleanup();
  });

  it('renders exactly one link to /sessions (no duplicate header link)', async () => {
    setupFetch({ detail: env(sessionDetail), graph: env(graph), events: env(eventsPayload) });
    rendered('aac68973');
    await waitFor(() => {
      const sessionLinks = screen
        .getAllByRole('link')
        .filter((a) => a.getAttribute('href') === '/sessions');
      expect(sessionLinks).toHaveLength(1);
    });
  });

  it('exposes named grid slots for kpi, stream, and detail (timeline removed)', async () => {
    setupFetch({ detail: env(sessionDetail), graph: env(graph), events: env(eventsPayload) });
    const { container } = rendered('aac68973');
    await waitFor(() => expect(screen.getByText(/2 events/)).toBeInTheDocument());
    expect(container.querySelector('[data-slot="kpi"]')).not.toBeNull();
    expect(container.querySelector('[data-slot="stream"]')).not.toBeNull();
    expect(container.querySelector('[data-slot="detail"]')).not.toBeNull();
    expect(container.querySelector('[data-slot="timeline"]')).toBeNull();
  });

  it('does not render the Waterfall/Graph ViewToggle', async () => {
    setupFetch({ detail: env(sessionDetail), graph: env(graph), events: env(eventsPayload) });
    rendered('aac68973');
    await waitFor(() => expect(screen.getByText(/2 events/)).toBeInTheDocument());
    expect(screen.queryByRole('button', { name: /graph/i })).toBeNull();
    expect(screen.queryByRole('tab', { name: /graph/i })).toBeNull();
  });
});
