import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { listSessions, getSession, getGraph, getEventRaw, ApiError } from '../client';

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

  it('getSession returns SessionDetail', async () => {
    (fetch as unknown as ReturnType<typeof vi.fn>).mockResolvedValueOnce(
      ok({ meta: { generated_at: 'now' }, data: { session_id: 's1', summary: { event_count: 0, by_kind: {}, first_observed_at: 'a', last_observed_at: 'b' }, events: [] } })
    );
    const out = await getSession('s1');
    expect(out.session_id).toBe('s1');
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
