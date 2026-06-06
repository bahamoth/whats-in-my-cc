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
 * already handles `appendOne` correctly. On gap/resync, bulk caches are wiped.
 */
import { useQueryClient } from '@tanstack/react-query';
import type { QueryClient } from '@tanstack/react-query';
import { useEffect, useRef } from 'react';
import { useLiveStream, type LiveEnvelope } from '../hooks/useLiveStream';
import { sessionKeys } from './queries';

interface BridgeOpts {
  client?: QueryClient;
  /** Fires for every envelope after cache invalidation is queued. PR-3
   *  uses it to push the envelope into `useSessionWindow.appendOne`. */
  onEnvelope?: (env: LiveEnvelope) => void;
  /** Fires after gap-driven cache invalidations. PR-3 uses it to reload
   *  the windowed event buffer. */
  onGap?: (info: { dropped?: number }) => void;
  /** Fires after resync-driven cache invalidations. */
  onResync?: (info: { reason?: string }) => void;
}

export function useLiveStreamBridge(sessionId: string, opts: BridgeOpts = {}): void {
  const ctxClient = useQueryClient();
  const client = opts.client ?? ctxClient;
  // Keep callbacks in a ref so identity changes do not re-open the SSE
  // connection (see useLiveStream's identical pattern + comment).
  const cbsRef = useRef({
    onEnvelope: opts.onEnvelope,
    onGap: opts.onGap,
    onResync: opts.onResync,
  });
  useEffect(() => {
    cbsRef.current = {
      onEnvelope: opts.onEnvelope,
      onGap: opts.onGap,
      onResync: opts.onResync,
    };
  });

  useLiveStream({
    url: `/v1/stream?session=${encodeURIComponent(sessionId)}`,
    scope: sessionId,
    onEnvelope: (env: LiveEnvelope) => {
      cbsRef.current.onEnvelope?.(env);
    },
    onGap: (info) => {
      void client.invalidateQueries({ queryKey: sessionKeys.events(sessionId) });
      cbsRef.current.onGap?.(info);
    },
    onResync: (info) => {
      void client.invalidateQueries({ queryKey: sessionKeys.session(sessionId) });
      cbsRef.current.onResync?.(info);
    },
  });
}
