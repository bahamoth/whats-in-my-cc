import { useCallback, useEffect, useMemo, useRef } from 'react';
import { Link, useParams } from 'react-router-dom';
import { ApiError } from '../api/client';
import type { GraphPayload } from '../api/types';
import { MetaStrip } from '../components/MetaStrip';
import { DetailPanel } from '../components/replay/detail/DetailPanel';
import { Timeline } from '../components/replay/timeline/Timeline';
import { TopBar } from '../components/layout/TopBar';
import { InsightStrip } from '../components/replay/insight-strip/InsightStrip';
import { EpisodeStrip } from '../components/replay/EpisodeStrip';
import {
  ReplaySelectionProvider,
  useReplaySelection,
} from '../components/replay/selection/ReplaySelection';
import { useLiveStreamBridge } from '../lib/sse';
import {
  useSessionDetailQuery,
  useSessionGraphQuery,
  useEpisodesQuery,
  useFindingsQuery,
  useVerificationRunsQuery,
  useToolFailureSummaryQuery,
  useSessionUsageQuery,
  useUsageBaselineQuery,
  useEventRawQuery,
} from '../lib/queries';
import { useSessionWindow } from '../hooks/useSessionWindow';
import { ConversationStream } from '../components/replay/stream/ConversationStream';
import { buildStreamModel } from '../components/replay/stream/streamModel';
import { buildLlmRequestMetrics } from '../components/replay/stream/llmRequestMetrics';
import styles from './SessionDetailPage.module.css';

const EMPTY_GRAPH: GraphPayload = { nodes: [], edges: [] };

// Debounce window for SSE-driven backfill: an envelope burst collapses to one
// forward `?after=` page fetch (mirrors the bridge's graph-invalidate debounce).
const BACKFILL_DEBOUNCE_MS = 600;

function SessionDetailInner({ sessionId }: { sessionId: string }) {
  const sel = useReplaySelection();

  const detail = useSessionDetailQuery(sessionId);
  const graph = useSessionGraphQuery(sessionId);
  const episodes = useEpisodesQuery(sessionId);
  const findings = useFindingsQuery(sessionId);
  const verificationRuns = useVerificationRunsQuery(sessionId);
  const toolFailures = useToolFailureSummaryQuery(sessionId);
  const usage = useSessionUsageQuery(sessionId);
  const baseline = useUsageBaselineQuery();

  const window_ = useSessionWindow(sessionId);

  // SSE envelopes are lightweight notifications WITHOUT a payload — appending
  // them directly would yield content-less events that the stream model drops
  // (the "live messages don't appear until refresh" bug). Instead, an envelope
  // schedules a debounced forward backfill that fetches the real, full-payload
  // events via `?after=`. A burst collapses to one fetch.
  const backfillTimerRef = useRef<number | null>(null);
  useEffect(
    () => () => {
      if (backfillTimerRef.current !== null) {
        clearTimeout(backfillTimerRef.current);
        backfillTimerRef.current = null;
      }
    },
    [],
  );
  useLiveStreamBridge(sessionId, {
    onEnvelope: () => {
      if (backfillTimerRef.current !== null) return;
      backfillTimerRef.current = setTimeout(() => {
        backfillTimerRef.current = null;
        void window_.loadNewer();
      }, BACKFILL_DEBOUNCE_MS) as unknown as number;
    },
    onGap: () => void window_.reload(),
    onResync: () => void window_.reload(),
  });

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

  const effectiveGraph = graph.data ?? EMPTY_GRAPH;

  // Findings drive the stream highlight + DetailPanel cross-reference (below).
  const findingsData = findings.data ?? [];

  // Per-response (LLM request) metrics, joined to thinking events by
  // request_id — the marker shows duration+tokens, selecting shows the rest.
  const metricsByReq = useMemo(
    () => buildLlmRequestMetrics(window_.events),
    [window_.events],
  );

  const streamItems = useMemo(
    () => buildStreamModel(window_.events, metricsByReq),
    [window_.events, metricsByReq],
  );

  // event_id -> node_id (graph nodes carry source_event_ids)
  const nodeIdByEventId = useMemo(() => {
    const m = new Map<string, string>();
    for (const n of effectiveGraph.nodes) for (const eid of n.source_event_ids) m.set(eid, n.node_id);
    return m;
  }, [effectiveGraph]);

  // event ids that have a finding. Evidence refs (slice-14/15+) live in two
  // namespaces; resolve each through its own path:
  //   - candidateNodeIds: refs that may be node ids (bare-string ULID refs and
  //     { node_id } refs) → mapped via the graph to their source event ids;
  //   - directEventIds: ids that are already event ids (bare-string refs and
  //     { event_id } refs) → used as-is.
  // A bare string ref is ambiguous, so it is treated as both.
  const findingEventIds = useMemo(() => {
    const candidateNodeIds = new Set<string>();
    const directEventIds = new Set<string>();
    for (const f of findingsData) {
      for (const ref of f.evidence_refs) {
        if (typeof ref === 'string') {
          candidateNodeIds.add(ref);
          directEventIds.add(ref);
        } else {
          if (typeof ref.node_id === 'string') candidateNodeIds.add(ref.node_id);
          if (typeof ref.event_id === 'string') {
            directEventIds.add(ref.event_id);
            // object refs with only an event_id were historically matched
            // against node ids too; preserve that.
            if (typeof ref.node_id !== 'string') candidateNodeIds.add(ref.event_id);
          }
        }
      }
    }
    const eids = new Set<string>(directEventIds);
    for (const n of effectiveGraph.nodes) {
      if (candidateNodeIds.has(n.node_id)) for (const eid of n.source_event_ids) eids.add(eid);
    }
    return eids;
  }, [findingsData, effectiveGraph]);

  // event_id -> episode phase (by observed_at within [started_at, ended_at]).
  // Computed over ALL window events — activity events (not just messages) need
  // phases now so ActivityStack can split runs by phase.
  const phaseByEventId = useMemo(() => {
    const eps = episodes.data ?? [];
    const out = new Map<string, string>();
    for (const ev of window_.events) {
      const t = ev.observed_at;
      const ep = eps.find((e) => e.started_at <= t && t <= e.ended_at);
      if (ep) out.set(ev.event_id, ep.phase);
    }
    return out;
  }, [window_.events, episodes.data]);

  const phaseOf = useCallback(
    (eventId: string) => phaseByEventId.get(eventId) ?? null,
    [phaseByEventId],
  );

  const selectedStreamEventId = useMemo(() => {
    if (!sel.selectedNodeId) return null;
    const n = effectiveGraph.nodes.find((x) => x.node_id === sel.selectedNodeId);
    // Graph nodes resolve to their first source event; nodeless selections
    // (e.g. a thinking marker) store the event id directly in `selected`.
    return n?.source_event_ids[0] ?? sel.selectedNodeId;
  }, [sel.selectedNodeId, effectiveGraph]);

  const selectStreamCard = (eventId: string) => {
    const nid = nodeIdByEventId.get(eventId);
    // Thinking events are not graph nodes; select them by event id so the
    // side panel can still resolve and show their per-response metrics.
    sel.setSelectedNodeId(nid ?? eventId);
  };

  // A selected thinking event (no graph node) → its per-response metrics for
  // the side panel.
  const selectedThinkingEvent = useMemo(() => {
    if (!sel.selectedNodeId) return null;
    return (
      window_.events.find(
        (e) => e.event_id === sel.selectedNodeId && e.kind === 'thinking',
      ) ?? null
    );
  }, [sel.selectedNodeId, window_.events]);
  const selectedThinkingMetrics = useMemo(() => {
    const rid = selectedThinkingEvent?.request_id ?? null;
    return rid ? metricsByReq.get(rid) ?? null : null;
  }, [selectedThinkingEvent, metricsByReq]);

  // --- DetailPanel inputs ---
  const selectedNode =
    sel.selectedNodeId && effectiveGraph
      ? effectiveGraph.nodes.find((n) => n.node_id === sel.selectedNodeId) ?? null
      : null;
  const selectedEventId = selectedNode?.source_event_ids[0] ?? null;

  const rawQuery = useEventRawQuery(selectedEventId);

  const selectedNodeFindings = useMemo(() => {
    if (!sel.selectedNodeId) return [];
    const nid = sel.selectedNodeId;
    const node = effectiveGraph.nodes.find((n) => n.node_id === nid);
    const sourceEventIds = new Set(node?.source_event_ids ?? []);
    return findingsData.filter((f) =>
      f.evidence_refs.some((ref) => {
        if (typeof ref === 'string') return ref === nid || sourceEventIds.has(ref);
        return ref.node_id === nid || (typeof ref.event_id === 'string' && sourceEventIds.has(ref.event_id));
      }),
    );
  }, [sel.selectedNodeId, effectiveGraph, findingsData]);

  const selectedNodePhase = useMemo(() => {
    if (!selectedNode) return null;
    const eps = episodes.data ?? [];
    const t = selectedNode.started_at;
    return eps.find((e) => e.started_at <= t && t <= e.ended_at)?.phase ?? null;
  }, [selectedNode, episodes.data]);

  // --- render branches ---
  const detailError = detail.error as ApiError | null;
  const is404 = detailError instanceof ApiError && detailError.status === 404;
  const isLoading = detail.isLoading;

  return (
    <div className={styles.page}>
      <TopBar sessionId={sessionId} />

      {isLoading && <p>Loading…</p>}

      {is404 && (
        <p>
          Session not found. <Link to="/sessions">Back to list</Link>
        </p>
      )}

      {!isLoading && !is404 && detail.data && (
        <div className={styles.grid} data-witmcc-detail-grid>
          <div className={styles.kpi} data-slot="kpi">
            <InsightStrip
              usage={usage.data}
              verificationRuns={verificationRuns.data}
              findings={findings.data}
              toolFailures={toolFailures.data}
              baseline={
                baseline.data
                  ? { cache_hit_ratio: baseline.data.cache_hit_ratio.median }
                  : undefined
              }
            />
            <EpisodeStrip episodes={episodes.data ?? []} />
            <MetaStrip session={detail.data} events={window_.events} />
          </div>

          <div className={styles.stream} data-slot="stream">
            {/* Sentinel parked at the top of the stream slot: scrolling up to here triggers loadOlder. */}
            <div
              ref={sentinelRef}
              aria-hidden
              style={{ height: 1 }}
              data-testid="scroll-sentinel"
            />
            <ConversationStream
              items={streamItems}
              phaseOf={phaseOf}
              selectedEventId={selectedStreamEventId}
              findingEventIds={findingEventIds}
              onSelect={selectStreamCard}
            />
          </div>

          <div className={styles.detail} data-slot="detail">
            <DetailPanel
              node={selectedNode}
              record={rawQuery.data?.record ?? null}
              findings={selectedNodeFindings}
              episodePhase={selectedNodePhase}
              nodes={effectiveGraph.nodes}
              edges={effectiveGraph.edges}
              onSelectNode={(id) => sel.setSelectedNodeId(id)}
              thinkingSelected={!!selectedThinkingEvent}
              thinkingMetrics={selectedThinkingMetrics}
            />
          </div>

          <div className={styles.timeline} data-slot="timeline">
            <Timeline
              nodes={effectiveGraph.nodes}
              edges={effectiveGraph.edges}
              episodes={episodes.data ?? []}
              selectedNodeId={sel.selectedNodeId}
              onSelect={(id) => sel.setSelectedNodeId(id)}
            />
          </div>
        </div>
      )}

      {!isLoading && !is404 && !detail.data && detail.isError && (
        <p role="alert">{detail.error?.message ?? 'failed'}</p>
      )}
    </div>
  );
}

export default function SessionDetailPage() {
  const { sessionId = '' } = useParams();
  return (
    <ReplaySelectionProvider>
      <SessionDetailInner sessionId={sessionId} />
    </ReplaySelectionProvider>
  );
}
