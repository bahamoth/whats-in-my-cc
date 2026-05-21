import { useCallback, useEffect, useRef, useState } from 'react';
import { Link, useParams } from 'react-router-dom';
import { ApiError, getGraph, getSession } from '../api/client';
import type { GraphPayload, SessionDetail } from '../api/types';
import { MetaStrip } from '../components/MetaStrip';
import { SourcePanel } from '../components/SourcePanel';
import { Timeline } from '../components/Timeline';
import { useLiveStream } from '../hooks/useLiveStream';
import styles from './SessionDetailPage.module.css';

// Live update throttle. Without this, a session with rapid OTel/log/transcript
// activity (e.g. claude code mid-tool-call emitting tens of envelopes per
// second) makes onEnvelope → refetch fire continuously. Each refetch returns
// up to 5000 events; React re-renders the entire timeline on every setState
// and the main thread freezes within seconds. Verified live on bahamoth's
// 4000+ event session, which froze the renderer until a hard reload.
//
// Trailing debounce: every envelope arms a 250ms timer; subsequent envelopes
// inside that window reuse the existing timer. When the timer fires we run
// exactly one refetch. New envelopes that arrive during the fetch itself
// arm a fresh timer for the next refetch, so we never queue refetches.
const LIVE_REFETCH_DEBOUNCE_MS = 250;

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

  // slice-8 — live updates with trailing debounce (see constant above).
  const debounceTimer = useRef<number | null>(null);
  const queueRefetch = useCallback(() => {
    if (debounceTimer.current !== null) return;
    debounceTimer.current = window.setTimeout(() => {
      debounceTimer.current = null;
      void refetch();
    }, LIVE_REFETCH_DEBOUNCE_MS);
  }, [refetch]);
  useEffect(() => {
    return () => {
      if (debounceTimer.current !== null) {
        window.clearTimeout(debounceTimer.current);
        debounceTimer.current = null;
      }
    };
  }, []);

  useLiveStream({
    url: `/v1/stream?session=${encodeURIComponent(sessionId)}`,
    scope: sessionId,
    onEnvelope: queueRefetch,
    onResync: queueRefetch,
    onGap: queueRefetch,
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
