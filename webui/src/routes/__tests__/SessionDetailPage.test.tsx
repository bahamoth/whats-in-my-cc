import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor, fireEvent, cleanup } from '@testing-library/react';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import '@testing-library/jest-dom/vitest';
import SessionDetailPage from '../SessionDetailPage';

function rendered(sessionId: string) {
  return render(
    <MemoryRouter initialEntries={[`/sessions/${sessionId}`]}>
      <Routes>
        <Route path="/sessions/:sessionId" element={<SessionDetailPage />} />
      </Routes>
    </MemoryRouter>,
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

  it('renders meta strip + timeline + empty SourcePanel hint', async () => {
    setupFetch({ detail: env(sessionDetail), graph: env(graph), events: env(eventsPayload) });
    rendered('s1');
    await waitFor(() => expect(screen.getByText(/2 events/)).toBeInTheDocument());
    expect(screen.getByText(/Click a node/i)).toBeInTheDocument();
  });

  it('clicking a node fetches raw and renders SourcePanel content', async () => {
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
    await waitFor(() => expect(screen.getByText('/tmp/a.jsonl')).toBeInTheDocument());
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
    expect(screen.getByText('Intent')).toBeInTheDocument();
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
