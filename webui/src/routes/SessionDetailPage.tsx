import { useCallback, useEffect, useState } from 'react';
import { Link, useParams } from 'react-router-dom';
import { ApiError, getGraph, getSession } from '../api/client';
import type { GraphPayload, SessionDetail } from '../api/types';
import { MetaStrip } from '../components/MetaStrip';
import { SourcePanel } from '../components/SourcePanel';
import { Timeline } from '../components/Timeline';
import { useLiveStream } from '../hooks/useLiveStream';
import styles from './SessionDetailPage.module.css';

type Loaded = { session: SessionDetail; graph: GraphPayload };
type State =
  | { kind: 'loading' }
  | { kind: 'ok'; data: Loaded }
  | { kind: 'not_found' }
  | { kind: 'error'; message: string };

export default function SessionDetailPage() {
  const { sessionId = '' } = useParams();
  const [state, setState] = useState<State>({ kind: 'loading' });
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);

  const refetch = useCallback(async () => {
    try {
      const session = await getSession(sessionId);
      let graph: GraphPayload;
      try {
        graph = await getGraph(sessionId);
      } catch (e) {
        if (e instanceof ApiError && e.status === 404) graph = { nodes: [], edges: [] };
        else throw e;
      }
      setState({ kind: 'ok', data: { session, graph } });
    } catch (e: unknown) {
      if (e instanceof ApiError && e.status === 404) setState({ kind: 'not_found' });
      else setState({ kind: 'error', message: e instanceof Error ? e.message : String(e) });
    }
  }, [sessionId]);

  useEffect(() => {
    let cancelled = false;
    setState({ kind: 'loading' });
    refetch().then(() => {
      if (cancelled) setState({ kind: 'loading' });
    });
    return () => {
      cancelled = true;
    };
  }, [refetch]);

  // slice-8 — live updates. Refetch on each envelope for this session.
  // Naive but correct: an envelope means a new row landed, and sqlite is
  // local so the refetch is cheap. Incremental append would be ~50 LOC and
  // is a follow-up if the chattiness is ever an issue.
  useLiveStream({
    url: `/v1/stream?session=${encodeURIComponent(sessionId)}`,
    scope: sessionId,
    onEnvelope: () => {
      void refetch();
    },
    onResync: () => {
      void refetch();
    },
    onGap: () => {
      void refetch();
    },
  });

  const selectedEventId =
    state.kind === 'ok' && selectedNodeId
      ? state.data.graph.nodes.find((n) => n.node_id === selectedNodeId)
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
          <MetaStrip session={state.data.session} />
          <div className={styles.split}>
            <Timeline
              graph={state.data.graph}
              selectedNodeId={selectedNodeId}
              onSelect={setSelectedNodeId}
            />
            <SourcePanel eventId={selectedEventId} />
          </div>
        </>
      )}
    </div>
  );
}
