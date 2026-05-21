import { useCallback, useEffect, useRef, useState } from 'react';
import { Link, useParams } from 'react-router-dom';
import { ApiError, getGraph, getSession } from '../api/client';
import type {
  GraphPayload,
  ObservedEventDto,
  SessionDetail,
} from '../api/types';
import { MetaStrip } from '../components/MetaStrip';
import { SourcePanel } from '../components/SourcePanel';
import { Timeline } from '../components/Timeline';
import { useLiveStream, type LiveEnvelope } from '../hooks/useLiveStream';
import { useSessionWindow } from '../hooks/useSessionWindow';
import styles from './SessionDetailPage.module.css';

// Slice-9 — graph refetch throttle. Replaces the slice-8 1000ms debounce on
// "refetch everything" (DEV-S8-13). Now only the graph is re-fetched; the
// summary refetch is interval-driven (see GRAPH_REFETCH_TICK_MS), and the
// per-event marker arrives via SSE without any backend round-trip.
const GRAPH_REFETCH_DEBOUNCE_MS = 800;
const SUMMARY_REFETCH_TICK_MS = 10_000;

type PageState =
  | { kind: 'loading' }
  | { kind: 'ok'; session: SessionDetail; graph: GraphPayload }
  | { kind: 'not_found' }
  | { kind: 'error'; message: string };

function envelopeToObserved(env: LiveEnvelope): ObservedEventDto {
  return {
    event_id: env.event_id,
    raw_event_id: '',
    session_id: env.session_id,
    event_uuid: null,
    parent_uuid: null,
    observed_at: env.observed_at,
    actor: 'unknown',
    kind: env.kind,
    subkind: null,
    tool_use_id: null,
    tool_name: null,
    turn_id: null,
    is_sidechain: false,
    is_meta: false,
    payload: {},
  };
}

export default function SessionDetailPage() {
  const { sessionId = '' } = useParams();
  const [state, setState] = useState<PageState>({ kind: 'loading' });
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);

  const window_ = useSessionWindow(sessionId);

  // Mount: fetch summary + graph in parallel. 404 on summary → not_found.
  // 404 on graph alone falls back to an empty graph (slice-8 DEV-S8-12 — the
  // empty-graph race during rebuild is now atomic but the *empty session*
  // case still legitimately returns an empty graph).
  const fetchAll = useCallback(async () => {
    try {
      const session = await getSession(sessionId);
      let graph: GraphPayload;
      try {
        graph = await getGraph(sessionId);
      } catch (e) {
        if (e instanceof ApiError && e.status === 404) graph = { nodes: [], edges: [] };
        else throw e;
      }
      setState({ kind: 'ok', session, graph });
    } catch (e: unknown) {
      if (e instanceof ApiError && e.status === 404) setState({ kind: 'not_found' });
      else setState({ kind: 'error', message: e instanceof Error ? e.message : String(e) });
    }
  }, [sessionId]);

  useEffect(() => {
    setState({ kind: 'loading' });
    void fetchAll();
  }, [fetchAll]);

  // Lightweight graph refetch on envelope arrival. We never re-fetch the
  // session detail or the full event window — those land via summary tick
  // and SSE-driven `appendOne` respectively. The debounce avoids stacking
  // graph fetches during a rapid envelope burst (claude code mid-tool-call
  // can emit dozens per second).
  const graphTimer = useRef<number | null>(null);
  const queueGraphRefetch = useCallback(() => {
    if (graphTimer.current !== null) return;
    graphTimer.current = window.setTimeout(() => {
      graphTimer.current = null;
      void getGraph(sessionId)
        .then((graph) =>
          setState((prev) => (prev.kind === 'ok' ? { ...prev, graph } : prev)),
        )
        .catch((e) => {
          if (!(e instanceof ApiError && e.status === 404)) {
            // transient — leave previous graph on screen
          }
        });
    }, GRAPH_REFETCH_DEBOUNCE_MS);
  }, [sessionId]);

  // Summary refetch on a slow interval so MetaStrip (event_count, last_observed_at,
  // per-kind counts) stays accurate without a backend round trip per envelope.
  useEffect(() => {
    if (state.kind !== 'ok') return;
    const id = window.setInterval(() => {
      void getSession(sessionId)
        .then((session) =>
          setState((prev) => (prev.kind === 'ok' ? { ...prev, session } : prev)),
        )
        .catch(() => {
          /* swallow transient */
        });
    }, SUMMARY_REFETCH_TICK_MS);
    return () => window.clearInterval(id);
  }, [sessionId, state.kind]);

  useEffect(() => {
    return () => {
      if (graphTimer.current !== null) {
        window.clearTimeout(graphTimer.current);
        graphTimer.current = null;
      }
    };
  }, []);

  useLiveStream({
    url: `/v1/stream?session=${encodeURIComponent(sessionId)}`,
    scope: sessionId,
    onEnvelope: (env) => {
      window_.appendOne(envelopeToObserved(env));
      queueGraphRefetch();
    },
    onResync: () => {
      void window_.reload();
      queueGraphRefetch();
    },
    onGap: () => {
      void window_.reload();
      queueGraphRefetch();
    },
  });

  // IntersectionObserver on the left-edge sentinel triggers `loadOlder` when
  // the user scrolls (or the Timeline's container brings the sentinel into
  // view). 0.5 intersection ratio keeps us from firing on hairline overlap
  // during initial render.
  const sentinelRef = useRef<HTMLDivElement | null>(null);
  useEffect(() => {
    const el = sentinelRef.current;
    if (!el) return;
    const io = new IntersectionObserver(
      (entries) => {
        const e = entries[0];
        if (e && e.intersectionRatio >= 0.5) {
          void window_.loadOlder();
        }
      },
      { threshold: [0, 0.5, 1] },
    );
    io.observe(el);
    return () => io.disconnect();
  }, [window_]);

  const selectedEventId =
    state.kind === 'ok' && selectedNodeId
      ? state.graph.nodes.find((n) => n.node_id === selectedNodeId)
          ?.source_event_ids[0] ?? null
      : null;

  return (
    <div className={styles.page}>
      <header className={styles.header}>
        <Link to="/sessions">← Sessions</Link>
        <code>{sessionId}</code>
      </header>
      {state.kind === 'loading' && <p>Loading…</p>}
      {state.kind === 'not_found' && (
        <p>Session not found. <Link to="/sessions">Back to list</Link></p>
      )}
      {state.kind === 'error' && <p role="alert">{state.message}</p>}
      {state.kind === 'ok' && (
        <>
          <MetaStrip session={state.session} events={window_.events} />
          <div className={styles.split}>
            <div>
              <div ref={sentinelRef} aria-hidden style={{ height: 1 }} data-testid="scroll-sentinel" />
              <Timeline
                graph={state.graph}
                selectedNodeId={selectedNodeId}
                onSelect={setSelectedNodeId}
              />
            </div>
            <SourcePanel eventId={selectedEventId} />
          </div>
        </>
      )}
    </div>
  );
}
