import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import {
  listSessions,
  getSession,
  getGraph,
  getEventRaw,
  getSessionEvents,
  ApiError,
} from '../client';

describe('api client', () => {
  beforeEach(() => {
    vi.stubGlobal('fetch', vi.fn());
  });
  afterEach(() => { vi.unstubAllGlobals(); });

  function ok(body: unknown) {
    return new Response(JSON.stringify(body), {
      status: 200, headers: { 'content-type': 'application/json' },
    });
  }

  it('listSessions unwraps envelope', async () => {
    (fetch as unknown as ReturnType<typeof vi.fn>).mockResolvedValueOnce(
      ok({ meta: { generated_at: 'now' }, data: [{ session_id: 's1', first_observed_at: 'a', last_observed_at: 'b', event_count: 3, source_uris: [] }] })
    );
    const out = await listSessions();
    expect(out[0].session_id).toBe('s1');
  });

  it('getSession returns SessionDetail (slice-9 — summary only)', async () => {
    const f = fetch as unknown as ReturnType<typeof vi.fn>;
    f.mockResolvedValueOnce(
      ok({ meta: { generated_at: 'now' }, data: { session_id: 's1', summary: { event_count: 0, by_kind: {}, first_observed_at: 'a', last_observed_at: 'b' } } })
    );
    const out = await getSession('s1');
    expect(out.session_id).toBe('s1');
    expect(f).toHaveBeenCalledWith('/v1/sessions/s1', expect.any(Object));
  });

  it('getSessionEvents builds /events URL with cursor params', async () => {
    const f = fetch as unknown as ReturnType<typeof vi.fn>;
    f.mockResolvedValueOnce(
      ok({ meta: { generated_at: 'n' }, data: { events: [], prev_cursor: null, next_cursor: null } })
    );
    await getSessionEvents('sess-A', { before: '2026-05-21T00:00:00Z|01J', limit: 200 });
    expect(f).toHaveBeenCalledWith(
      '/v1/sessions/sess-A/events?before=2026-05-21T00%3A00%3A00Z%7C01J&limit=200',
      expect.any(Object),
    );
  });

  it('getSessionEvents with no opts hits /events without query string', async () => {
    const f = fetch as unknown as ReturnType<typeof vi.fn>;
    f.mockResolvedValueOnce(
      ok({ meta: { generated_at: 'n' }, data: { events: [], prev_cursor: null, next_cursor: null } })
    );
    await getSessionEvents('x');
    expect(f).toHaveBeenCalledWith('/v1/sessions/x/events', expect.any(Object));
  });

  it('getEventRaw throws ApiError on 404', async () => {
    (fetch as unknown as ReturnType<typeof vi.fn>).mockResolvedValueOnce(
      new Response('{"detail":"event x not found"}', { status: 404 })
    );
    await expect(getEventRaw('x')).rejects.toBeInstanceOf(ApiError);
  });

  it('getGraph uses /v1/sessions/:id/graph', async () => {
    const f = fetch as unknown as ReturnType<typeof vi.fn>;
    f.mockResolvedValueOnce(ok({ meta: {generated_at:'n'}, data: { nodes: [], edges: [] } }));
    await getGraph('abc');
    expect(f).toHaveBeenCalledWith('/v1/sessions/abc/graph', expect.any(Object));
  });
});
