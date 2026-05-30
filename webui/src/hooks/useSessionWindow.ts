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
import { getSessionEvents } from '../api/client';
import type { ObservedEventDto } from '../api/types';

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
  /** Push a single event (typically an SSE envelope). Dedupes by event_id and
   *  drops rows whose `(observed_at, event_id)` is ≤ the newest already in
   *  the window. */
  appendOne: (e: ObservedEventDto) => void;
  /** Force re-initial-fetch (e.g. after SSE `event: resync`). */
  reload: () => Promise<void>;
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

  // Loading ref so concurrent `appendOne` / `loadOlder` see fresh state
  // without waiting for React's setState batch to commit.
  const loadingRef = useRef<WindowLoadingState>('initial');
  const setLoadingBoth = useCallback((s: WindowLoadingState) => {
    loadingRef.current = s;
    setLoading(s);
  }, []);

  const doInitial = useCallback(async () => {
    setLoadingBoth('initial');
    setError(null);
    try {
      const resp = await getSessionEvents(sessionId, { limit: initialLimit });
      setEvents(resp.events);
      setOldest(resp.prev_cursor);
      setNewest(resp.next_cursor);
      setAtLiveTip(resp.next_cursor === null);
      setLoadingBoth('idle');
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
      setLoadingBoth('error');
    }
  }, [sessionId, initialLimit, setLoadingBoth]);

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
      });
      if (resp.events.length === 0) {
        setOldest(null);
        setLoadingBoth('idle');
        return;
      }
      setEvents((prev) => [...resp.events, ...prev]);
      setOldest(resp.prev_cursor);
      setLoadingBoth('idle');
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
      setLoadingBoth('idle'); // recoverable — page can retry
    }
  }, [sessionId, oldest, pageLimit, setLoadingBoth]);

  const loadNewer = useCallback(async () => {
    if (loadingRef.current !== 'idle') return;
    const cur = eventsRef.current;
    if (cur.length === 0) return;
    const after = cursorOf(cur[cur.length - 1]);
    setLoadingBoth('newer');
    try {
      const resp = await getSessionEvents(sessionId, { after, limit: pageLimit });
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
      setLoadingBoth('idle');
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
      setLoadingBoth('idle'); // recoverable — next envelope retries
    }
  }, [sessionId, pageLimit, maxEvents, setLoadingBoth]);

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
    appendOne,
    reload: doInitial,
  };
}
