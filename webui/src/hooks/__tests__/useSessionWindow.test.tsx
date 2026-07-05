import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { useSessionWindow } from '../useSessionWindow';
import type { ObservedEventDto } from '../../api/types';
import type { EventFilterParams } from '../../components/replay/stream/filterState';

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

  it('reload() loads the TAIL even when mounted with initialAround (resume-follow → latest)', async () => {
    // A `?selected=` deep-link mounts AROUND the target (detached). When the
    // reader toggles autoscroll ON, the page calls reload() to catch up to the
    // live tip — it MUST fetch the latest tail, NOT re-issue ?around= (which
    // would strand them on the deep-link slice instead of going to latest).
    const f = fetch as unknown as ReturnType<typeof vi.fn>;
    f.mockResolvedValueOnce(
      envelope({
        events: [makeEvent(10), makeEvent(11)],
        prev_cursor: 'pc-around',
        next_cursor: 'nc-around',
      }),
    );
    const { result } = renderHook(() =>
      useSessionWindow('s', { initialAround: makeEvent(10).event_id }),
    );
    await waitFor(() => expect(result.current.loading).toBe('idle'));
    // mount used ?around=
    expect(f.mock.calls.at(-1)?.[0] as string).toContain(
      `around=${encodeURIComponent(makeEvent(10).event_id)}`,
    );
    expect(result.current.atLiveTip).toBe(false);

    f.mockResolvedValueOnce(
      envelope({
        events: [makeEvent(98), makeEvent(99)],
        prev_cursor: 'pc-tail',
        next_cursor: null,
      }),
    );
    await act(async () => {
      await result.current.reload();
    });
    const reloadUrl = f.mock.calls.at(-1)?.[0] as string;
    expect(reloadUrl).not.toContain('around=');
    expect(result.current.events.map((e) => e.event_id)).toEqual([
      makeEvent(98).event_id,
      makeEvent(99).event_id,
    ]);
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

  // --- loadAround — deep-link `?selected=<event_id>` outside the loaded
  // window. The client only has the event_id (no observed_at → no cursor), so
  // it asks the server for the window AROUND that event and REPLACES the
  // buffer with it. prev/next cursors come from the response, so older/newer
  // pagination keeps working from the new window.
  it('loadAround replaces the window with the around page and keeps cursors', async () => {
    const f = fetch as unknown as ReturnType<typeof vi.fn>;
    f.mockResolvedValueOnce(
      envelope({ events: [makeEvent(50), makeEvent(51)], prev_cursor: 'pc-tail', next_cursor: null }),
    );
    const { result } = renderHook(() => useSessionWindow('s'));
    await waitFor(() => expect(result.current.loading).toBe('idle'));

    f.mockResolvedValueOnce(
      envelope({
        events: [makeEvent(9), makeEvent(10), makeEvent(11)],
        prev_cursor: 'pc-around',
        next_cursor: 'nc-around',
      }),
    );
    let found: boolean | undefined;
    await act(async () => {
      found = await result.current.loadAround(makeEvent(10).event_id);
    });
    expect(found).toBe(true);
    // window REPLACED (not appended): tail rows are gone, around rows in.
    expect(result.current.events.map((e) => e.event_id)).toEqual([
      makeEvent(9).event_id,
      makeEvent(10).event_id,
      makeEvent(11).event_id,
    ]);
    expect(result.current.oldest).toBe('pc-around');
    expect(result.current.newest).toBe('nc-around');
    expect(result.current.atLiveTip).toBe(false);
    // fetch used ?around=<event_id>
    const lastCall = f.mock.calls.at(-1)?.[0] as string;
    expect(lastCall).toContain(`around=${encodeURIComponent(makeEvent(10).event_id)}`);
  });

  it('loadAround returns false on 404 and leaves the window intact', async () => {
    const f = fetch as unknown as ReturnType<typeof vi.fn>;
    f.mockResolvedValueOnce(
      envelope({ events: [makeEvent(1), makeEvent(2)], prev_cursor: 'pc', next_cursor: null }),
    );
    const { result } = renderHook(() => useSessionWindow('s'));
    await waitFor(() => expect(result.current.loading).toBe('idle'));

    f.mockResolvedValueOnce(
      new Response('{"detail":"event nope not found in session s"}', { status: 404 }),
    );
    let found: boolean | undefined;
    await act(async () => {
      found = await result.current.loadAround('nope');
    });
    expect(found).toBe(false);
    expect(result.current.events).toHaveLength(2);
    expect(result.current.oldest).toBe('pc');
    expect(result.current.loading).toBe('idle');
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

  // --- Task 8: filter threading — §1.4. All three fetch paths (initial tail,
  // loadOlder, loadNewer) must carry the active filter's query params, and a
  // `filterKey` change must reset the buffer (re-run the initial fetch).
  it('passes filter params to tail/older/newer fetches and resets on filterKey change', async () => {
    const f = fetch as unknown as ReturnType<typeof vi.fn>;
    const filter: EventFilterParams = { origin: 'human', q: 'deploy' };

    f.mockResolvedValueOnce(
      envelope({ events: [makeEvent(10), makeEvent(11)], prev_cursor: 'pc-init', next_cursor: null }),
    );
    const { result, rerender } = renderHook(
      ({ fk }: { fk: string }) => useSessionWindow('s1', { filter, filterKey: fk }),
      { initialProps: { fk: 'A' } },
    );
    await waitFor(() => expect(result.current.loading).toBe('idle'));
    // initial tail fetch includes filter params
    const initUrl = f.mock.calls.at(-1)?.[0] as string;
    expect(initUrl).toContain(`origin=${encodeURIComponent('human')}`);
    expect(initUrl).toContain(`q=${encodeURIComponent('deploy')}`);

    f.mockResolvedValueOnce(
      envelope({ events: [makeEvent(5)], prev_cursor: 'pc-older', next_cursor: null }),
    );
    await act(async () => {
      await result.current.loadOlder();
    });
    const olderUrl = f.mock.calls.at(-1)?.[0] as string;
    expect(olderUrl).toContain('before=');
    expect(olderUrl).toContain(`q=${encodeURIComponent('deploy')}`);

    // filterKey 변경 → 버퍼 리셋(재-initial fetch)
    const callsBefore = f.mock.calls.length;
    f.mockResolvedValueOnce(
      envelope({ events: [makeEvent(20)], prev_cursor: 'pc-reset', next_cursor: null }),
    );
    rerender({ fk: 'B' });
    await waitFor(() => expect(f.mock.calls.length).toBeGreaterThan(callsBefore));
    // 리셋은 호출 수만이 아니라 버퍼도 교체한다 — 새 filterKey 응답(makeEvent(20))이
    // 실제로 events에 반영되고 이전 윈도우(10/11 + older 5)는 사라져야 한다.
    await waitFor(() =>
      expect(result.current.events.map((e) => e.event_id)).toEqual([makeEvent(20).event_id]),
    );
  });

  it('keeps matchedCount null when no filter is active', async () => {
    // §1.4 doc invariant: matchedCount is null when no filter is active. The
    // real backend contract omits `matched_count` from the response unless a
    // filter is present (docs/specs §1.2), so an unfiltered tail fetch must
    // leave matchedCount at null — never a stray 0/total. Locks against a
    // regression to `resp.matched_count ?? 0` (undefined ?? 0 = 0 ≠ null).
    const f = fetch as unknown as ReturnType<typeof vi.fn>;
    f.mockResolvedValueOnce(
      envelope({ events: [makeEvent(1), makeEvent(2)], prev_cursor: 'pc', next_cursor: null }),
    );
    const { result } = renderHook(() => useSessionWindow('s1'));
    await waitFor(() => expect(result.current.loading).toBe('idle'));
    expect(result.current.events).toHaveLength(2); // fetch resolved
    expect(result.current.matchedCount).toBeNull();
  });

  it('does not re-fetch when filterKey is unchanged (no reset loop)', async () => {
    // The reset trigger is filter *identity* (filterKey), not object churn.
    // Re-rendering with the same filter reference AND the same filterKey must
    // NOT re-run the initial fetch — otherwise a parent re-render would loop
    // the buffer. Locks the "리셋 안 되는 절반".
    const f = fetch as unknown as ReturnType<typeof vi.fn>;
    const filter: EventFilterParams = { q: 'deploy' };
    f.mockResolvedValue(
      envelope({ events: [makeEvent(1)], prev_cursor: 'pc', next_cursor: null }),
    );
    const { result, rerender } = renderHook(
      ({ fk }: { fk: string }) => useSessionWindow('s1', { filter, filterKey: fk }),
      { initialProps: { fk: 'A' } },
    );
    await waitFor(() => expect(result.current.loading).toBe('idle'));
    const callsAfterInit = f.mock.calls.length;

    // Same filterKey (and same filter reference) → no new fetch.
    rerender({ fk: 'A' });
    // Flush any pending effect/microtask so a spurious refetch would surface.
    await act(async () => {
      await Promise.resolve();
    });
    expect(f.mock.calls.length).toBe(callsAfterInit);
  });

  it('exposes matched_count from the response', async () => {
    const f = fetch as unknown as ReturnType<typeof vi.fn>;
    f.mockResolvedValueOnce(
      envelope({ events: [], prev_cursor: null, next_cursor: null, matched_count: 42 }),
    );
    const { result } = renderHook(() =>
      useSessionWindow('s1', { filter: { q: 'x' }, filterKey: 'k' }),
    );
    await waitFor(() => expect(result.current.matchedCount).toBe(42));
  });

  // --- Task 11 defence: deep-link (initialAround) × active filter. The backend
  // 400s on around×filter (§1.2), so a deep-link mount under an ACTIVE filter
  // must NOT issue an unfiltered `?around=` fetch (which would render an
  // unfiltered window while buildStreamModel is already in flat/filter mode).
  // The filter wins: discard the around-window and load the FILTERED TAIL.
  it('with initialAround AND an active filter, loads the filtered tail (not the around window)', async () => {
    const f = fetch as unknown as ReturnType<typeof vi.fn>;
    // Stable filter reference (the page memoizes it by filterKey); an inlined
    // object would churn identity and re-run the initial effect every render.
    const filter: EventFilterParams = { q: 'deploy' };
    f.mockResolvedValueOnce(
      envelope({
        events: [makeEvent(20), makeEvent(21)],
        prev_cursor: 'pc-filtered',
        next_cursor: null,
        matched_count: 2,
      }),
    );
    const { result } = renderHook(() =>
      useSessionWindow('s1', {
        initialAround: makeEvent(10).event_id,
        filter,
        filterKey: 'k',
      }),
    );
    await waitFor(() => expect(result.current.loading).toBe('idle'));
    const initUrl = f.mock.calls.at(-1)?.[0] as string;
    // filtered tail, NOT the around window
    expect(initUrl).not.toContain('around=');
    expect(initUrl).toContain(`q=${encodeURIComponent('deploy')}`);
    expect(result.current.events.map((e) => e.event_id)).toEqual([
      makeEvent(20).event_id,
      makeEvent(21).event_id,
    ]);
    expect(result.current.matchedCount).toBe(2);
  });

  it('with initialAround and NO filter, still loads the around window (no regression)', async () => {
    const f = fetch as unknown as ReturnType<typeof vi.fn>;
    f.mockResolvedValueOnce(
      envelope({ events: [makeEvent(9), makeEvent(10)], prev_cursor: 'pc', next_cursor: 'nc' }),
    );
    const { result } = renderHook(() =>
      useSessionWindow('s1', { initialAround: makeEvent(10).event_id }),
    );
    await waitFor(() => expect(result.current.loading).toBe('idle'));
    const initUrl = f.mock.calls.at(-1)?.[0] as string;
    expect(initUrl).toContain(`around=${encodeURIComponent(makeEvent(10).event_id)}`);
  });

  // Browser smoke (2026-07-05, session 653ea169) caught this live: jump-to-event
  // clears the filter (SessionDetailPage's jumpNeedsFilterClear), which — with
  // `initialAround` still set — re-enters doInitial's AROUND branch (filter now
  // falsy). That branch never called `setMatchedCount`, so the stale N from the
  // prior FILTERED tail fetch kept rendering ("N건 매칭") under the now-cleared
  // filter indefinitely (until some other fetch path happened to run). The
  // around branch is unfiltered by construction (`!filter` is its own guard),
  // so matchedCount must reset to null once it resolves.
  it('clears a stale matchedCount when a jump-triggered filter clear falls into the around branch', async () => {
    const f = fetch as unknown as ReturnType<typeof vi.fn>;
    const filter: EventFilterParams | null = { q: 'deploy' };
    // 1) Initial mount: filter active → filtered tail, matched_count present.
    f.mockResolvedValueOnce(
      envelope({ events: [makeEvent(1)], prev_cursor: 'pc', next_cursor: null, matched_count: 985 }),
    );
    const { result, rerender } = renderHook<
      ReturnType<typeof useSessionWindow>,
      { filter: EventFilterParams | null }
    >(
      ({ filter: fl }) =>
        useSessionWindow('s1', {
          initialAround: makeEvent(10).event_id,
          filter: fl,
          filterKey: fl ? 'k' : '',
        }),
      { initialProps: { filter } },
    );
    await waitFor(() => expect(result.current.matchedCount).toBe(985));

    // 2) Jump clears the filter (SessionDetailPage.applyFilter(EMPTY_FILTER)) —
    // filterKey changes, initialAround is still set → falls into the AROUND
    // branch (unfiltered by construction).
    f.mockResolvedValueOnce(
      envelope({ events: [makeEvent(9), makeEvent(10)], prev_cursor: 'pc2', next_cursor: 'nc2' }),
    );
    rerender({ filter: null });
    await waitFor(() => {
      const lastUrl = f.mock.calls.at(-1)?.[0] as string;
      expect(lastUrl).toContain('around=');
    });
    await waitFor(() => expect(result.current.matchedCount).toBeNull());
  });
});
