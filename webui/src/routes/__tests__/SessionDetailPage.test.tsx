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
  episodes?: Response;
  findings?: Response;
};

function setupFetch(routes: Routes) {
  const fn = vi.fn((input: RequestInfo | URL) => {
    const url = typeof input === 'string' ? input : input.toString();
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
    if (url.includes('/episodes')) {
      if (routes.episodes) return Promise.resolve(routes.episodes.clone());
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
// source_event_ids (n1→ev1, n2→ev2). Used to exercise timeline↔stream
// cross-sync end-to-end (empty `eventsPayload` mounts zero StreamCards and
// therefore cannot prove the wiring).
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

  it('renders meta strip + timeline + DetailPanel empty hint before node selection', async () => {
    setupFetch({ detail: env(sessionDetail), graph: env(graph), events: env(eventsPayload) });
    rendered('s1');
    await waitFor(() => expect(screen.getByText(/2 events/)).toBeInTheDocument());
    expect(screen.getByText(/select a node to inspect it/i)).toBeInTheDocument();
  });

  it('clicking a node shows the DetailPanel tablist', async () => {
    setupFetch({
      detail: env(sessionDetail),
      graph: env(graph),
      events: env(eventsPayload),
      raw: env(raw),
    });
    rendered('s1');
    const marker = await waitFor(() => {
      const el = document.querySelector('[data-node-id="n1"]');
      if (!el) throw new Error('marker not found');
      return el;
    });
    fireEvent.click(marker);
    await waitFor(() => expect(screen.getByRole('tablist')).toBeInTheDocument());
    expect(screen.getByRole('tab', { name: /insight/i })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: /raw/i })).toBeInTheDocument();
    expect(screen.queryByRole('tab', { name: /^detail$/i })).toBeNull();
  });

  // The headline cross-sync requirement (spec §2/§3/§5.7): selection is
  // bidirectionally synced across the timeline and the conversation stream,
  // and a node click brings the matching stream card into view. Every other
  // integration test uses an empty events payload, so this is the ONLY place
  // the timeline↔stream wiring is proven end-to-end with real cards mounted.
  it('cross-syncs selection between timeline nodes and stream cards', async () => {
    const scrollSpy = vi
      .spyOn(Element.prototype, 'scrollIntoView')
      .mockImplementation(() => {});
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

    // Timeline node → stream: clicking node n1 selects the ev1 card AND
    // scrolls it into view.
    const node1 = container.querySelector('[data-node-id="n1"]');
    expect(node1).not.toBeNull();
    fireEvent.click(node1!);
    await waitFor(() => {
      const card1 = container.querySelector(
        '[data-event-id="ev1"] [data-testid="message-card"]',
      );
      expect(card1?.getAttribute('data-selected')).toBe('true');
    });
    expect(scrollSpy).toHaveBeenCalled();

    // Stream → timeline: clicking the ev2 card moves selection to node n2.
    const card2 = container.querySelector(
      '[data-event-id="ev2"] [data-testid="message-card"]',
    );
    expect(card2).not.toBeNull();
    fireEvent.click(card2!);
    await waitFor(() => {
      expect(
        container.querySelector('[data-node-id="n2"][data-selected="true"]'),
      ).not.toBeNull();
    });
    // ...and selection has moved off ev1.
    expect(
      container
        .querySelector('[data-event-id="ev1"] [data-testid="message-card"]')
        ?.getAttribute('data-selected'),
    ).toBe('false');

    scrollSpy.mockRestore();
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

  it('renders empty timeline when getGraph 404s but getSession succeeds', async () => {
    setupFetch({
      detail: env(sessionDetail),
      graph: new Response('{"detail":"no graph"}', { status: 404 }),
      events: env(eventsPayload),
    });
    rendered('s1');
    await waitFor(() => expect(screen.getByText(/2 events/)).toBeInTheDocument());
    expect(screen.queryByText(/session not found/i)).not.toBeInTheDocument();
    // Timeline still renders (its SVG canvas) even with an empty graph; lane
    // rows are now hidden when empty (#4), so assert the canvas, not a lane.
    expect(screen.getByTestId('timeline-canvas')).toBeInTheDocument();
  });

  // Slice-9 — envelope-driven append + debounced graph refetch. Verifies
  // that an envelope burst inside the debounce window collapses to ONE
  // graph fetch (not one per envelope as slice-8 did) and that neither the
  // summary nor the events endpoint is re-hit per envelope. This is the
  // integration lock for DEV-S8-13 fix.
  it('envelope burst triggers a single debounced graph refetch (not per-envelope)', async () => {
    MockEventSource.install();
    const f = setupFetch({
      detail: env(sessionDetail),
      graph: env(graph),
      events: env(eventsPayload),
    });
    rendered('s1');
    await waitFor(() => expect(screen.getByText(/2 events/)).toBeInTheDocument());

    // Snapshot mount-time fetch counts.
    const callsOf = (matcher: (u: string) => boolean) =>
      f.mock.calls.filter((c) => matcher(String(c[0]))).length;
    const summaryAtMount = callsOf((u) => /\/v1\/sessions\/[^/]+$/.test(u));
    const graphAtMount = callsOf((u) => u.includes('/graph'));
    const eventsAtMount = callsOf((u) => u.includes('/events'));
    expect(summaryAtMount).toBe(1);
    expect(graphAtMount).toBe(1);
    expect(eventsAtMount).toBe(1);

    // Fire 5 envelopes in tight succession. They should all `appendOne`
    // into useSessionWindow synchronously and arm exactly one graph
    // debounce timer.
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

    // Wait for the queued fetch to resolve. The graph endpoint should now
    // have been hit exactly once more than at mount; summary and events
    // unchanged.
    await waitFor(() => {
      expect(callsOf((u) => u.includes('/graph'))).toBe(graphAtMount + 1);
    });
    expect(callsOf((u) => /\/v1\/sessions\/[^/]+$/.test(u))).toBe(summaryAtMount);
    expect(callsOf((u) => u.includes('/events'))).toBe(eventsAtMount);
  });

  // Slice-9 — IntersectionObserver-driven loadOlder. The mounted sentinel
  // must exist in the DOM; we observe via a fake IO that fires immediately
  // and assert getSessionEvents is called with `?before=...`.
  it('IntersectionObserver triggers loadOlder when sentinel intersects', async () => {
    const f = setupFetch({
      detail: env(sessionDetail),
      graph: env(graph),
      events: env({
        events: [],
        prev_cursor: '2026-05-19T10:00:00Z|01J',
        next_cursor: null,
      }),
    });

    // Install a fake IntersectionObserver that triggers on observe().
    let triggered = false;
    class FakeIO {
      cb: IntersectionObserverCallback;
      constructor(cb: IntersectionObserverCallback) { this.cb = cb; }
      observe(_el: Element) {
        // Defer to allow the page to finish initial fetches before
        // triggering loadOlder.
        setTimeout(() => {
          if (triggered) return;
          triggered = true;
          this.cb(
            [{ isIntersecting: true, intersectionRatio: 1 } as IntersectionObserverEntry],
            this as unknown as IntersectionObserver,
          );
        }, 0);
      }
      disconnect() {}
      unobserve() {}
      root = null;
      rootMargin = '';
      thresholds = [];
      takeRecords() { return []; }
    }
    vi.stubGlobal('IntersectionObserver', FakeIO);

    rendered('s1');
    await waitFor(() => expect(screen.getByText(/2 events/)).toBeInTheDocument());
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

  it('exposes named grid slots for kpi, stream, detail, and timeline', async () => {
    setupFetch({ detail: env(sessionDetail), graph: env(graph), events: env(eventsPayload) });
    const { container } = rendered('aac68973');
    await waitFor(() => expect(screen.getByText(/2 events/)).toBeInTheDocument());
    expect(container.querySelector('[data-slot="kpi"]')).not.toBeNull();
    expect(container.querySelector('[data-slot="stream"]')).not.toBeNull();
    expect(container.querySelector('[data-slot="detail"]')).not.toBeNull();
    expect(container.querySelector('[data-slot="timeline"]')).not.toBeNull();
  });

  it('does not render the Waterfall/Graph ViewToggle', async () => {
    setupFetch({ detail: env(sessionDetail), graph: env(graph), events: env(eventsPayload) });
    rendered('aac68973');
    await waitFor(() => expect(screen.getByText(/2 events/)).toBeInTheDocument());
    expect(screen.queryByRole('button', { name: /graph/i })).toBeNull();
    expect(screen.queryByRole('tab', { name: /graph/i })).toBeNull();
  });
});
