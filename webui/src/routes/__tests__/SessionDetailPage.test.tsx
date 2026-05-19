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

const sessionDetail = {
  session_id: 's1',
  summary: {
    event_count: 2,
    by_kind: { user_message: 1, assistant_message: 1 },
    first_observed_at: '2026-05-19T10:00:00Z',
    last_observed_at: '2026-05-19T10:00:05Z',
  },
  events: [],
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
  beforeEach(() => { vi.stubGlobal('fetch', vi.fn()); });
  afterEach(() => { vi.unstubAllGlobals(); cleanup(); });

  it('renders meta strip + timeline + empty SourcePanel hint', async () => {
    const f = fetch as unknown as ReturnType<typeof vi.fn>;
    f.mockResolvedValueOnce(env(sessionDetail));
    f.mockResolvedValueOnce(env(graph));
    rendered('s1');
    await waitFor(() => expect(screen.getByText(/2 events/)).toBeInTheDocument());
    expect(screen.getByText(/Click a node/i)).toBeInTheDocument();
  });

  it('clicking a node fetches raw and renders SourcePanel content', async () => {
    const f = fetch as unknown as ReturnType<typeof vi.fn>;
    f.mockResolvedValueOnce(env(sessionDetail));
    f.mockResolvedValueOnce(env(graph));
    rendered('s1');
    const marker = await waitFor(() => {
      const el = document.querySelector('[data-node-id="n1"]');
      if (!el) throw new Error('marker not found');
      return el;
    });
    f.mockResolvedValueOnce(env(raw));
    fireEvent.click(marker);
    await waitFor(() => expect(screen.getByText('/tmp/a.jsonl')).toBeInTheDocument());
  });

  it('shows 404 when session detail missing', async () => {
    const f = fetch as unknown as ReturnType<typeof vi.fn>;
    f.mockResolvedValueOnce(new Response('{"detail":"session nope not found"}', { status: 404 }));
    f.mockResolvedValueOnce(new Response('{"detail":"no graph"}', { status: 404 }));
    rendered('nope');
    await waitFor(() => expect(screen.getByText(/session not found/i)).toBeInTheDocument());
  });
});
