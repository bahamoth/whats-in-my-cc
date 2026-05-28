/**
 * PR-2 — SSE → React Query bridge.
 *
 * The backend emits three kinds of SSE frames (`src/api/sse.rs`):
 *   - default `message` — an ObservedEvent envelope (`kind`, `event_id`, …)
 *   - `event: gap`     — broadcast lagged, frontend should refetch baseline
 *   - `event: resync`  — server cursor invalidated, frontend should wipe all
 *
 * This bridge translates frames into cache invalidations on the
 * `sessionKeys.*` keys defined in `queries.ts`. The bridge does NOT touch
 * the `events` window — that is owned by `useSessionWindow` (slice-9), which
 * already handles `appendOne` correctly. We only re-validate the *graph*
 * and (on gap/resync) the bulk caches.
 */
import { useQueryClient } from '@tanstack/react-query';
import type { QueryClient } from '@tanstack/react-query';
import { useEffect, useRef } from 'react';
import { useLiveStream, type LiveEnvelope } from '../hooks/useLiveStream';
import { sessionKeys } from './queries';

const GRAPH_INVALIDATE_DEBOUNCE_MS = 800;

interface BridgeOpts {
  client?: QueryClient;
  graphDebounceMs?: number;
}

export function useLiveStreamBridge(sessionId: string, opts: BridgeOpts = {}): void {
  const ctxClient = useQueryClient();
  const client = opts.client ?? ctxClient;
  const debounceMs = opts.graphDebounceMs ?? GRAPH_INVALIDATE_DEBOUNCE_MS;
  const timerRef = useRef<number | null>(null);

  useEffect(() => {
    return () => {
      if (timerRef.current !== null) {
        clearTimeout(timerRef.current);
        timerRef.current = null;
      }
    };
  }, []);

  const queueGraphInvalidate = () => {
    if (timerRef.current !== null) return;
    timerRef.current = setTimeout(() => {
      timerRef.current = null;
      void client.invalidateQueries({ queryKey: sessionKeys.graph(sessionId) });
    }, debounceMs) as unknown as number;
  };

  useLiveStream({
    url: `/v1/stream?session=${encodeURIComponent(sessionId)}`,
    scope: sessionId,
    onEnvelope: (_env: LiveEnvelope) => {
      queueGraphInvalidate();
    },
    onGap: () => {
      void client.invalidateQueries({ queryKey: sessionKeys.events(sessionId) });
      void client.invalidateQueries({ queryKey: sessionKeys.graph(sessionId) });
    },
    onResync: () => {
      void client.invalidateQueries({ queryKey: sessionKeys.session(sessionId) });
    },
  });
}
