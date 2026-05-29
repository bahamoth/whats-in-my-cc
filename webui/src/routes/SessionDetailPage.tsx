import { useEffect, useMemo, useRef } from 'react';
import { Link, useParams } from 'react-router-dom';
import { ApiError } from '../api/client';
import type { GraphPayload, ObservedEventDto } from '../api/types';
import { MetaStrip } from '../components/MetaStrip';
import { DetailPanel } from '../components/replay/detail/DetailPanel';
import { Timeline } from '../components/replay/timeline/Timeline';
import { TopBar } from '../components/layout/TopBar';
import { KpiStrip } from '../components/replay/KpiStrip';
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
  useDiffHunksQuery,
  useEventRawQuery,
} from '../lib/queries';
import { useSessionWindow } from '../hooks/useSessionWindow';
import type { LiveEnvelope } from '../hooks/useLiveStream';
import { ConversationStream } from '../components/replay/stream/ConversationStream';
import { buildStreamCards } from '../components/replay/stream/streamModel';
import styles from './SessionDetailPage.module.css';

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

const EMPTY_GRAPH: GraphPayload = { nodes: [], edges: [] };

function SessionDetailInner({ sessionId }: { sessionId: string }) {
  const sel = useReplaySelection();

  const detail = useSessionDetailQuery(sessionId);
  const graph = useSessionGraphQuery(sessionId);
  const episodes = useEpisodesQuery(sessionId);
  const findings = useFindingsQuery(sessionId);
  const verificationRuns = useVerificationRunsQuery(sessionId);
  const diffHunks = useDiffHunksQuery(sessionId);

  const window_ = useSessionWindow(sessionId);

  useLiveStreamBridge(sessionId, {
    onEnvelope: (env) => window_.appendOne(envelopeToObserved(env)),
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

  // --- KPI strip inputs ---
  const findingsData = findings.data ?? [];
  const riskCount = useMemo(
    () => findingsData.filter((f) => f.severity === 'high').length,
    [findingsData],
  );
  const verificationCoverage = useMemo(() => {
    const total = diffHunks.data?.length ?? 0;
    const covered = new Set<string>();
    for (const vr of verificationRuns.data ?? []) {
      for (const h of vr.covered_diff_hunk_ids) covered.add(h);
    }
    return total === 0 ? null : { covered: covered.size, total };
  }, [diffHunks.data, verificationRuns.data]);

  const outcome: 'clean' | 'attention' | 'problem' | 'unknown' = useMemo(() => {
    if (findingsData.length === 0) return 'clean';
    if (findingsData.some((f) => f.severity === 'high')) return 'problem';
    if (findingsData.some((f) => f.severity === 'medium')) return 'attention';
    return 'clean';
  }, [findingsData]);

  const streamCards = useMemo(() => buildStreamCards(window_.events), [window_.events]);

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

  // event_id -> episode phase (by observed_at within [started_at, ended_at])
  const phaseByEventId = useMemo(() => {
    const eps = episodes.data ?? [];
    const out: Record<string, string> = {};
    for (const card of streamCards) {
      const t = card.timestamp;
      const ep = eps.find((e) => e.started_at <= t && t <= e.ended_at);
      if (ep) out[card.eventId] = ep.phase;
    }
    return out;
  }, [streamCards, episodes.data]);

  const selectedStreamEventId = useMemo(() => {
    if (!sel.selectedNodeId) return null;
    const n = effectiveGraph.nodes.find((x) => x.node_id === sel.selectedNodeId);
    return n?.source_event_ids[0] ?? null;
  }, [sel.selectedNodeId, effectiveGraph]);

  const selectStreamCard = (eventId: string) => {
    const nid = nodeIdByEventId.get(eventId);
    sel.setSelectedNodeId(nid ?? null);
  };

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
            <KpiStrip
              outcome={outcome}
              verificationCoverage={verificationCoverage}
              episodeCount={episodes.data?.length ?? 0}
              riskCount={riskCount}
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
              cards={streamCards}
              selectedEventId={selectedStreamEventId}
              phaseByEventId={phaseByEventId}
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
