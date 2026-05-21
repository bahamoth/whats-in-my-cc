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
// 1000ms — 250ms was too aggressive for 4000+ event sessions where each
// refetch carries ~200 KB JSON + a Timeline re-render over thousands of
// SVG circles. Verified: with 250ms the renderer froze within seconds of
// active claude code activity. 1000ms keeps the UI responsive and the
// "live" feeling is still well within human perception.
const LIVE_REFETCH_DEBOUNCE_MS = 1000;

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
      // Anti-flicker + anti-thrash:
      // (a) If the new graph came back empty but the previous render had a
      //     non-empty graph, treat as a transient race during ingest's
      //     graph::build::rebuild_session and KEEP the previous graph.
      // (b) If nothing structural changed (same node/edge counts, same event
      //     count), return the prev reference. React skips re-render on
      //     identity equality, so Timeline does not re-layout 3000+ SVG
      //     circles for no reason.
      setState((prev) => {
        if (
          graph.nodes.length === 0 &&
          prev.kind === 'ok' &&
          prev.data.graph.nodes.length > 0
        ) {
          return { kind: 'ok', data: { session, graph: prev.data.graph } };
        }
        if (
          prev.kind === 'ok' &&
          prev.data.graph.nodes.length === graph.nodes.length &&
          prev.data.graph.edges.length === graph.edges.length &&
          prev.data.session.summary.event_count === session.summary.event_count &&
          // Also compare the latest event_id in the events window — when the
          // session has more events than the 5000-cap, event_count keeps
          // advancing on the server but the page's window count stays the
          // same; without this extra check we would skip re-render and the
          // newest envelope would never reach the Timeline.
          prev.data.session.events[prev.data.session.events.length - 1]?.event_id ===
            session.events[session.events.length - 1]?.event_id
        ) {
          return prev;
        }
        return { kind: 'ok', data: { session, graph } };
      });
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
