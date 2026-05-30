import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { useSessionWindow } from '../useSessionWindow';
import type { ObservedEventDto } from '../../api/types';

function makeEvent(i: number): ObservedEventDto {
  return {
    event_id: `01J${String(i).padStart(23, '0')}`,
    raw_event_id: `raw_${i}`,
    session_id: 's',
    event_uuid: `u${i}`,
    parent_uuid: null,
    observed_at: `2026-05-21T00:00:${String(i).padStart(2, '0')}Z`,
    actor: 'user',
    kind: 'user_message',
    subkind: null,
    tool_use_id: null,
    tool_name: null,
    turn_id: null,
    is_sidechain: false,
    is_meta: false,
    payload: {},
  };
}

function envelope(data: unknown) {
  return new Response(JSON.stringify({ meta: { generated_at: 'n' }, data }), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  });
}

describe('useSessionWindow', () => {
  beforeEach(() => {
    vi.stubGlobal('fetch', vi.fn());
  });
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('initial fetch populates events + sets atLiveTip when next_cursor null', async () => {
    const f = fetch as unknown as ReturnType<typeof vi.fn>;
    const initEvents = [makeEvent(1), makeEvent(2), makeEvent(3)];
    f.mockResolvedValueOnce(
      envelope({
        events: initEvents,
        prev_cursor: '2026-05-21T00:00:00Z|01J' + '0'.repeat(23),
        next_cursor: null,
      }),
    );
    const { result } = renderHook(() => useSessionWindow('s', { initialLimit: 3 }));
    await waitFor(() => expect(result.current.loading).toBe('idle'));
    expect(result.current.events).toHaveLength(3);
    expect(result.current.atLiveTip).toBe(true);
    expect(result.current.oldest).not.toBeNull();
    expect(result.current.newest).toBeNull();
  });

  it('loadOlder prepends an older window and updates oldest cursor', async () => {
    const f = fetch as unknown as ReturnType<typeof vi.fn>;
    f.mockResolvedValueOnce(
      envelope({
        events: [makeEvent(10), makeEvent(11)],
        prev_cursor: '2026-05-21T00:00:10Z|01J' + '0'.repeat(20) + '010',
        next_cursor: null,
      }),
    );
    const { result } = renderHook(() => useSessionWindow('s'));
    await waitFor(() => expect(result.current.loading).toBe('idle'));

    f.mockResolvedValueOnce(
      envelope({
        events: [makeEvent(5), makeEvent(6)],
        prev_cursor: '2026-05-21T00:00:05Z|01J' + '0'.repeat(20) + '005',
        next_cursor: '2026-05-21T00:00:06Z|01J' + '0'.repeat(20) + '006',
      }),
    );
    await act(async () => {
      await result.current.loadOlder();
    });
    expect(result.current.events).toHaveLength(4);
    expect(result.current.events[0].event_id).toContain('00000005');
    expect(result.current.events[3].event_id).toContain('00000011');
  });

  it('appendOne pushes a strictly-newer event and updates newest', async () => {
    const f = fetch as unknown as ReturnType<typeof vi.fn>;
    f.mockResolvedValueOnce(
      envelope({
        events: [makeEvent(1), makeEvent(2)],
        prev_cursor: 'pc',
        next_cursor: null,
      }),
    );
    const { result } = renderHook(() => useSessionWindow('s'));
    await waitFor(() => expect(result.current.loading).toBe('idle'));
    act(() => {
      result.current.appendOne(makeEvent(3));
    });
    expect(result.current.events).toHaveLength(3);
    expect(result.current.events[2].event_id).toContain('00000003');
  });

  it('appendOne ignores duplicates by event_id', async () => {
    const f = fetch as unknown as ReturnType<typeof vi.fn>;
    f.mockResolvedValueOnce(
      envelope({
        events: [makeEvent(1), makeEvent(2)],
        prev_cursor: 'pc',
        next_cursor: null,
      }),
    );
    const { result } = renderHook(() => useSessionWindow('s'));
    await waitFor(() => expect(result.current.loading).toBe('idle'));
    act(() => {
      result.current.appendOne(makeEvent(2));
    });
    expect(result.current.events).toHaveLength(2);
  });

  it('appendOne ignores events older than the newest', async () => {
    const f = fetch as unknown as ReturnType<typeof vi.fn>;
    f.mockResolvedValueOnce(
      envelope({
        events: [makeEvent(5), makeEvent(6)],
        prev_cursor: 'pc',
        next_cursor: null,
      }),
    );
    const { result } = renderHook(() => useSessionWindow('s'));
    await waitFor(() => expect(result.current.loading).toBe('idle'));
    act(() => {
      // After a `next_cursor: null` initial fetch, `newest` is null and
      // appendOne accepts strictly-newer-than-null = everything. Push a
      // proper newer event first so newest advances.
      result.current.appendOne(makeEvent(7));
    });
    expect(result.current.events).toHaveLength(3);
    act(() => {
      result.current.appendOne(makeEvent(6)); // older than 7
    });
    expect(result.current.events).toHaveLength(3);
  });

  it('LRU evict followed by loadOlder fetches from the new oldest cursor', async () => {
    const f = fetch as unknown as ReturnType<typeof vi.fn>;
    f.mockResolvedValueOnce(
      envelope({
        events: [makeEvent(0), makeEvent(1)],
        prev_cursor: '2026-05-21T00:00:00Z|01J' + '0'.repeat(20) + '000',
        next_cursor: null,
      }),
    );
    const { result } = renderHook(() =>
      useSessionWindow('s', { initialLimit: 2, maxEvents: 3, pageLimit: 100 }),
    );
    await waitFor(() => expect(result.current.loading).toBe('idle'));

    // Burst past cap so trim fires and oldest cursor moves forward.
    act(() => {
      for (let i = 2; i < 10; i++) result.current.appendOne(makeEvent(i));
    });
    expect(result.current.events.length).toBeLessThanOrEqual(3);
    const evictedOldest = result.current.oldest;
    expect(evictedOldest).not.toBe('2026-05-21T00:00:00Z|01J' + '0'.repeat(20) + '000');
    expect(evictedOldest).not.toBeNull();

    // loadOlder must fetch with the NEW oldest cursor (evict-driven), not
    // the original prev_cursor. Mock returns a recognisable older page.
    f.mockResolvedValueOnce(
      envelope({
        events: [makeEvent(100)], // sentinel id we can spot
        prev_cursor: 'pc-after-evict-load',
        next_cursor: null,
      }),
    );
    await act(async () => {
      await result.current.loadOlder();
    });
    // The fetch URL must have used the post-evict oldest, not 'pc-original'.
    const lastCall = f.mock.calls.at(-1)?.[0] as string;
    expect(lastCall).toContain('?before=');
    expect(lastCall).toContain(encodeURIComponent(evictedOldest!));
    expect(result.current.events.some((e) => e.event_id.includes('00000100'))).toBe(true);
    expect(result.current.oldest).toBe('pc-after-evict-load');
  });

  it('reload() re-issues the initial fetch and resets cursors', async () => {
    const f = fetch as unknown as ReturnType<typeof vi.fn>;
    f.mockResolvedValueOnce(
      envelope({
        events: [makeEvent(1), makeEvent(2)],
        prev_cursor: 'pc-first',
        next_cursor: 'nc-first',
      }),
    );
    const { result } = renderHook(() => useSessionWindow('s'));
    await waitFor(() => expect(result.current.loading).toBe('idle'));
    expect(result.current.oldest).toBe('pc-first');
    expect(result.current.newest).toBe('nc-first');

    // appendOne to mutate state away from initial — reload must wipe this.
    act(() => {
      result.current.appendOne(makeEvent(99));
    });
    expect(result.current.events.length).toBe(3);

    f.mockResolvedValueOnce(
      envelope({
        events: [makeEvent(5)],
        prev_cursor: 'pc-reloaded',
        next_cursor: null,
      }),
    );
    await act(async () => {
      await result.current.reload();
    });
    expect(result.current.events).toHaveLength(1);
    expect(result.current.events[0].event_id).toContain('00000005');
    expect(result.current.oldest).toBe('pc-reloaded');
    expect(result.current.newest).toBeNull();
    expect(result.current.atLiveTip).toBe(true);
  });

  it('loadNewer fetches ?after=<last cursor> and appends the newer events', async () => {
    const f = fetch as unknown as ReturnType<typeof vi.fn>;
    f.mockResolvedValueOnce(
      envelope({ events: [makeEvent(1), makeEvent(2)], prev_cursor: 'pc', next_cursor: null }),
    );
    const { result } = renderHook(() => useSessionWindow('s'));
    await waitFor(() => expect(result.current.loading).toBe('idle'));

    f.mockResolvedValueOnce(
      envelope({ events: [makeEvent(3), makeEvent(4)], prev_cursor: 'x', next_cursor: null }),
    );
    await act(async () => {
      await result.current.loadNewer();
    });

    expect(result.current.events).toHaveLength(4);
    expect(result.current.events[3].event_id).toContain('00000004');
    expect(result.current.atLiveTip).toBe(true);
    // The fetch must page forward from the LAST loaded event's cursor.
    const lastCall = f.mock.calls.at(-1)?.[0] as string;
    expect(lastCall).toContain('after=');
    expect(lastCall).toContain(
      encodeURIComponent(`2026-05-21T00:00:02Z|${makeEvent(2).event_id}`),
    );
  });

  it('loadNewer dedupes events already present (after-cursor boundary overlap)', async () => {
    const f = fetch as unknown as ReturnType<typeof vi.fn>;
    f.mockResolvedValueOnce(
      envelope({ events: [makeEvent(1), makeEvent(2)], prev_cursor: 'pc', next_cursor: null }),
    );
    const { result } = renderHook(() => useSessionWindow('s'));
    await waitFor(() => expect(result.current.loading).toBe('idle'));

    // Server echoes event 2 (boundary overlap) plus a genuinely new event 3.
    f.mockResolvedValueOnce(
      envelope({ events: [makeEvent(2), makeEvent(3)], prev_cursor: 'x', next_cursor: null }),
    );
    await act(async () => {
      await result.current.loadNewer();
    });

    expect(result.current.events).toHaveLength(3);
    expect(result.current.events.map((e) => e.event_id).filter((id) => id.includes('00000002'))).toHaveLength(1);
  });

  it('loadNewer is a no-op when the window is empty', async () => {
    const f = fetch as unknown as ReturnType<typeof vi.fn>;
    f.mockResolvedValueOnce(
      envelope({ events: [], prev_cursor: null, next_cursor: null }),
    );
    const { result } = renderHook(() => useSessionWindow('s'));
    await waitFor(() => expect(result.current.loading).toBe('idle'));
    const callsBefore = f.mock.calls.length;
    await act(async () => {
      await result.current.loadNewer();
    });
    expect(f.mock.calls.length).toBe(callsBefore); // no fetch issued
  });

  it('LRU cap: appending past maxEvents trims oldest and shifts oldest cursor', async () => {
    const f = fetch as unknown as ReturnType<typeof vi.fn>;
    f.mockResolvedValueOnce(
      envelope({
        events: Array.from({ length: 4 }, (_, i) => makeEvent(i)),
        prev_cursor: 'pc-original',
        next_cursor: null,
      }),
    );
    const { result } = renderHook(() =>
      useSessionWindow('s', { initialLimit: 4, maxEvents: 5 }),
    );
    await waitFor(() => expect(result.current.loading).toBe('idle'));

    // Append until trim triggers (DEFAULT_TRIM = 500 in hook, so we set
    // maxEvents=5 to keep test fast; trim runs when next.length > maxEvents).
    // After cap, the hook drops `DEFAULT_TRIM = 500` rows. With only 6 rows
    // total, that empties the window — we instead assert the *behavior*:
    // events length must never exceed maxEvents, oldest cursor updates.
    act(() => {
      for (let i = 4; i < 510; i++) {
        result.current.appendOne(makeEvent(i));
      }
    });
    expect(result.current.events.length).toBeLessThanOrEqual(5);
    // oldest cursor must point at the (now) earliest row in the window, not
    // at the original page-1 prev_cursor.
    expect(result.current.oldest).not.toBe('pc-original');
  });
});
