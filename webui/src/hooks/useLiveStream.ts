// Slice-8 — React hook wrapping EventSource lifecycle for /v1/stream.
//
// Responsibilities:
// - Open EventSource on mount, close on unmount (route change / page exit).
// - Read sessionStorage cursor at mount, append as ?last_event_id= to URL.
// - On envelope: persist event_id to cursor, invoke onEnvelope().
// - On `event: gap`: invoke onGap, leave cursor intact (client refetches baseline).
// - On `event: resync`: clear cursor and invoke onResync (client wipes state).
//
// EventSource handles reconnection + Last-Event-ID header replay natively.

import { useEffect, useRef } from 'react';
import { readCursor, writeCursor, clearCursor } from '../api/cursor';

export interface LiveEnvelope {
  schema_version: string;
  session_id: string;
  event_id: string;
  kind: string;
  source_type: string;
  observed_at: string;
}

export interface UseLiveStreamArgs {
  url: string;
  scope: string;
  onEnvelope: (env: LiveEnvelope) => void;
  onGap?: (info: { dropped: number }) => void;
  onResync?: (info: { reason: string }) => void;
}

export function useLiveStream({
  url,
  scope,
  onEnvelope,
  onGap,
  onResync,
}: UseLiveStreamArgs): void {
  const esRef = useRef<EventSource | null>(null);

  // Callbacks are stored in a ref so the EventSource effect does not re-run
  // (and therefore close + reopen the connection) on every parent render. A
  // naive `[url, scope, onEnvelope, ...]` dep array makes every envelope
  // trigger setState → parent re-render → new inline callback references →
  // effect cleanup + re-run → connection thrash. Verified live on
  // bahamoth's WebUI: 44 EventSource entries observed within ~1.5s when
  // SessionDetailPage's `onEnvelope: () => void refetch()` was reading
  // `refetch` directly from dep array.
  const cbsRef = useRef({ onEnvelope, onGap, onResync });
  useEffect(() => {
    cbsRef.current = { onEnvelope, onGap, onResync };
  });

  useEffect(() => {
    const cursor = readCursor(scope);
    const fullUrl = cursor
      ? url + (url.includes('?') ? '&' : '?') + 'last_event_id=' + encodeURIComponent(cursor)
      : url;
    const es = new EventSource(fullUrl);
    esRef.current = es;

    es.onmessage = (ev: MessageEvent) => {
      try {
        const env: LiveEnvelope = JSON.parse(ev.data);
        writeCursor(scope, env.event_id);
        cbsRef.current.onEnvelope(env);
      } catch {
        /* ignore malformed frame */
      }
    };
    es.addEventListener('gap', (ev) => {
      try {
        const info = JSON.parse((ev as MessageEvent).data ?? '{}');
        cbsRef.current.onGap?.(info);
      } catch {
        /* ignore */
      }
    });
    es.addEventListener('resync', (ev) => {
      clearCursor(scope);
      try {
        const info = JSON.parse((ev as MessageEvent).data ?? '{}');
        cbsRef.current.onResync?.(info);
      } catch {
        /* ignore */
      }
    });

    return () => {
      es.close();
      esRef.current = null;
    };
  }, [url, scope]);
}
