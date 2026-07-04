// Slice-9 — windowed event buffer for SessionDetailPage. Replaces slice-8's
// `getSession(?limit=5000)` + refetch-the-world model with a video-streaming
// shaped cache: initial fetch loads the newest window, scroll-back fetches
// older windows on demand, SSE envelopes append at the live tip.
//
// Invariants:
//   - `events` is always sorted ASC by (observed_at, event_id).
//   - `oldest` is null iff the window reaches the session's earliest row.
//   - `atLiveTip === true` iff the window's newest row equals the session's
//     last_observed_at (server signals this with `next_cursor: null`).
//   - `events.length <= maxEvents`. When appending would overflow, we drop
//     the oldest `trim` rows and shift `oldest` forward to the new earliest
//     row's cursor (so subsequent scroll-back still has a starting point).
//
// Cursor format: `<observed_at_rfc3339>|<event_id>` (server contract).

import { useCallback, useEffect, useRef, useState } from 'react';
import { ApiError, getSessionEvents } from '../api/client';
import type { ObservedEventDto } from '../api/types';
import type { EventFilterParams } from '../components/replay/stream/filterState';

const DEFAULT_INITIAL_LIMIT = 500;
const DEFAULT_PAGE_LIMIT = 500;
const DEFAULT_MAX_EVENTS = 5000;
// LRU trim ratio — when `events.length` exceeds `maxEvents` we drop this
// fraction of the buffer from the oldest end. Ratio-based so it scales with
// `maxEvents`: a test using maxEvents=5 still trims one row, not 500.
const LRU_TRIM_RATIO = 0.1;

export type WindowLoadingState =
  | 'initial'
  | 'older'
  | 'newer'
  | 'idle'
  | 'error';

export interface UseSessionWindowOpts {
  initialLimit?: number;
  pageLimit?: number;
  maxEvents?: number;
  /** When mounting on a `?selected=` deep-link, load the window AROUND this event
   *  instead of the live tail — so the target is in the buffer from the first
   *  load and there is no tail-vs-around race (the live-session deep-link bug).
   *  Captured once at mount by the caller; must be stable. */
  initialAround?: string | null;
  /** Active event filter (§1.4). When set, ALL fetches (tail/older/newer)
   *  carry these params so the buffer only ever holds matching rows.
   *  `loadAround` never carries a filter — around×filter is unsupported
   *  (§1.2); callers must clear the filter before deep-linking. */
  filter?: EventFilterParams | null;
  /** Filter identity (order-invariant serialization, see `filterKey()` in
   *  filterState.ts). Changing this resets the buffer — the initial-fetch
   *  effect re-runs and REPLACES `events` with the newly filtered tail. */
  filterKey?: string;
}

export interface UseSessionWindowResult {
  events: ObservedEventDto[];
  oldest: string | null;
  newest: string | null;
  atLiveTip: boolean;
  loading: WindowLoadingState;
  error: string | null;
  /** Fetch the next older page using the current `oldest` cursor. No-op if
   *  loading, or if we already hit the start of the session. */
  loadOlder: () => Promise<void>;
  /** Fetch any events newer than the window's last row (forward `?after=`
   *  page) and append them with their full payloads. This is the live-tip
   *  path: an SSE envelope is a lightweight notification (no payload), so we
   *  backfill the real events instead of appending the empty envelope. No-op
   *  while another load is in flight or the window is empty. */
  loadNewer: () => Promise<void>;
  /** Deep-link jump: fetch the window AROUND the given event (`?around=`) and
   *  REPLACE the buffer with it. Used when `?selected=<event_id>` points at an
   *  event outside the loaded window — the client has no cursor for it (only
   *  the bare event_id). Cursors come from the response, so older/newer
   *  pagination keeps working from the new window. Resolves `false` (window
   *  untouched) when the event does not exist (404) or a load is in flight. */
  loadAround: (eventId: string) => Promise<boolean>;
  /** Push a single event (typically an SSE envelope). Dedupes by event_id and
   *  drops rows whose `(observed_at, event_id)` is ≤ the newest already in
   *  the window. */
  appendOne: (e: ObservedEventDto) => void;
  /** Force re-initial-fetch (e.g. after SSE `event: resync`). */
  reload: () => Promise<void>;
  /** Total rows matching the active filter across the whole session (not just
   *  the loaded window), from the newest tail/older/newer response's
   *  `matched_count`. `null` when no filter is active or none has loaded yet
   *  (never coerced to 0 — "unmeasured ≠ 0"). `loadAround` does not update
   *  this (around×filter unsupported). */
  matchedCount: number | null;
}

function cursorOf(e: ObservedEventDto): string {
  return `${e.observed_at}|${e.event_id}`;
}

function cursorTuple(c: string): [string, string] | null {
  const pipe = c.indexOf('|');
  if (pipe < 0) return null;
  return [c.slice(0, pipe), c.slice(pipe + 1)];
}

function isStrictlyNewerThan(e: ObservedEventDto, cur: string | null): boolean {
  if (cur === null) return true;
  const t = cursorTuple(cur);
  if (!t) return true;
  const [obs, eid] = t;
  if (e.observed_at > obs) return true;
  if (e.observed_at < obs) return false;
  return e.event_id > eid;
}

export function useSessionWindow(
  sessionId: string,
  opts: UseSessionWindowOpts = {},
): UseSessionWindowResult {
  const initialLimit = opts.initialLimit ?? DEFAULT_INITIAL_LIMIT;
  const pageLimit = opts.pageLimit ?? DEFAULT_PAGE_LIMIT;
  const maxEvents = opts.maxEvents ?? DEFAULT_MAX_EVENTS;
  const filter = opts.filter ?? null;
  const filterKey = opts.filterKey ?? '';

  const [events, setEvents] = useState<ObservedEventDto[]>([]);
  // Mirror of `events` so async `loadNewer` reads the freshest tail cursor
  // without a stale closure (it derives `?after=` from the last row).
  const eventsRef = useRef<ObservedEventDto[]>([]);
  eventsRef.current = events;
  const [oldest, setOldest] = useState<string | null>(null);
  const [newest, setNewest] = useState<string | null>(null);
  const [atLiveTip, setAtLiveTip] = useState<boolean>(false);
  const [loading, setLoading] = useState<WindowLoadingState>('initial');
  const [error, setError] = useState<string | null>(null);
  const [matchedCount, setMatchedCount] = useState<number | null>(null);

  // Loading ref so concurrent `appendOne` / `loadOlder` see fresh state
  // without waiting for React's setState batch to commit.
  const loadingRef = useRef<WindowLoadingState>('initial');
  const setLoadingBoth = useCallback((s: WindowLoadingState) => {
    loadingRef.current = s;
    setLoading(s);
  }, []);

  const initialAround = opts.initialAround ?? null;

  // Load the latest tail window and REPLACE the buffer with it. This is the
  // `reload` path (resume-follow catch-up): when the reader toggles autoscroll
  // ON, the page calls this to jump to the live tip — it must fetch the LATEST,
  // never the `initialAround` deep-link slice (which would strand them on the
  // around-window instead of going to latest — the "토글해도 latest로 안 옴" bug).
  const loadTail = useCallback(async () => {
    setLoadingBoth('initial');
    setError(null);
    try {
      const resp = await getSessionEvents(sessionId, {
        limit: initialLimit,
        ...(filter ? { filter } : {}),
      });
      setEvents(resp.events);
      setOldest(resp.prev_cursor);
      setNewest(resp.next_cursor);
      setAtLiveTip(resp.next_cursor === null);
      setMatchedCount(resp.matched_count ?? null);
      setLoadingBoth('idle');
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
      setLoadingBoth('error');
    }
    // `filterKey` (not just `filter`) is a dep so callers that memoize `filter`
    // by identity still get a fresh `loadTail`/`doInitial` (and thus a reset
    // fetch) exactly when the filter's *value* changes.
  }, [sessionId, initialLimit, filter, filterKey, setLoadingBoth]);

  const doInitial = useCallback(async () => {
    // Non-deep-link mount: the tail IS the initial window.
    if (!initialAround) {
      await loadTail();
      return;
    }
    // Deep-link mount: load AROUND the target so it is in the buffer from the
    // first load (no tail load to race/overwrite it). `reload` (loadTail) takes
    // over once the reader resumes following.
    setLoadingBoth('initial');
    setError(null);
    try {
      const resp = await getSessionEvents(sessionId, { around: initialAround, limit: initialLimit });
      setEvents(resp.events);
      setOldest(resp.prev_cursor);
      setNewest(resp.next_cursor);
      setAtLiveTip(resp.next_cursor === null);
      setLoadingBoth('idle');
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
      setLoadingBoth('error');
    }
    // `filterKey` dep: even though this branch's fetch doesn't carry `filter`
    // (around×filter unsupported, §1.2), a filterKey change must still
    // re-trigger `doInitial` via the effect below so the page can react
    // (e.g. clear the deep-link around-window once the caller drops `around`).
  }, [sessionId, initialLimit, initialAround, loadTail, filterKey, setLoadingBoth]);

  useEffect(() => {
    void doInitial();
  }, [doInitial]);

  const loadOlder = useCallback(async () => {
    if (loadingRef.current !== 'idle') return;
    if (oldest === null) return;
    setLoadingBoth('older');
    try {
      const resp = await getSessionEvents(sessionId, {
        before: oldest,
        limit: pageLimit,
        ...(filter ? { filter } : {}),
      });
      if (resp.events.length === 0) {
        setOldest(null);
        setMatchedCount(resp.matched_count ?? null);
        setLoadingBoth('idle');
        return;
      }
      setEvents((prev) => [...resp.events, ...prev]);
      setOldest(resp.prev_cursor);
      setMatchedCount(resp.matched_count ?? null);
      setLoadingBoth('idle');
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
      setLoadingBoth('idle'); // recoverable — page can retry
    }
  }, [sessionId, oldest, pageLimit, filter, setLoadingBoth]);

  const loadNewer = useCallback(async () => {
    if (loadingRef.current !== 'idle') return;
    const cur = eventsRef.current;
    if (cur.length === 0) return;
    const after = cursorOf(cur[cur.length - 1]);
    setLoadingBoth('newer');
    try {
      const resp = await getSessionEvents(sessionId, {
        after,
        limit: pageLimit,
        ...(filter ? { filter } : {}),
      });
      if (resp.events.length > 0) {
        setEvents((prev) => {
          const have = new Set(prev.map((p) => p.event_id));
          const fresh = resp.events.filter((e) => !have.has(e.event_id));
          if (fresh.length === 0) return prev;
          let next = [...prev, ...fresh];
          if (next.length > maxEvents) {
            const trim = Math.max(1, Math.floor(maxEvents * LRU_TRIM_RATIO));
            next = next.slice(trim);
            if (next.length > 0) setOldest(cursorOf(next[0]));
          }
          return next;
        });
      }
      setNewest(resp.next_cursor);
      setAtLiveTip(resp.next_cursor === null);
      setMatchedCount(resp.matched_count ?? null);
      setLoadingBoth('idle');
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
      setLoadingBoth('idle'); // recoverable — next envelope retries
    }
  }, [sessionId, pageLimit, maxEvents, filter, setLoadingBoth]);

  const loadAround = useCallback(
    async (eventId: string): Promise<boolean> => {
      if (loadingRef.current !== 'idle') return false;
      // 'older' so the existing "이전 메시지 불러오는 중…" affordance shows.
      setLoadingBoth('older');
      try {
        const resp = await getSessionEvents(sessionId, {
          around: eventId,
          limit: pageLimit,
        });
        setEvents(resp.events);
        setOldest(resp.prev_cursor);
        setNewest(resp.next_cursor);
        setAtLiveTip(resp.next_cursor === null);
        setLoadingBoth('idle');
        return true;
      } catch (e: unknown) {
        if (e instanceof ApiError && e.status === 404) {
          // Deep link to an event that no longer exists (retention sweep /
          // wrong session): keep the loaded window, just report not-found.
          setLoadingBoth('idle');
          return false;
        }
        setError(e instanceof Error ? e.message : String(e));
        setLoadingBoth('idle'); // recoverable
        return false;
      }
    },
    [sessionId, pageLimit, setLoadingBoth],
  );

  const appendOne = useCallback(
    (e: ObservedEventDto) => {
      setEvents((prev) => {
        if (!isStrictlyNewerThan(e, newest)) {
          return prev;
        }
        // dedupe in case a backfill ?after= overlapped the SSE envelope.
        if (prev.some((p) => p.event_id === e.event_id)) {
          return prev;
        }
        const next = [...prev, e];
        if (next.length > maxEvents) {
          const trim = Math.max(1, Math.floor(maxEvents * LRU_TRIM_RATIO));
          const trimmed = next.slice(trim);
          if (trimmed.length > 0) {
            setOldest(cursorOf(trimmed[0]));
          }
          return trimmed;
        }
        return next;
      });
      setNewest(cursorOf(e));
      setAtLiveTip(true);
    },
    [newest, maxEvents],
  );

  return {
    events,
    oldest,
    newest,
    atLiveTip,
    loading,
    error,
    loadOlder,
    loadNewer,
    loadAround,
    appendOne,
    reload: loadTail,
    matchedCount,
  };
}
