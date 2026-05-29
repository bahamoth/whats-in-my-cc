import { useEffect, useMemo, useRef } from 'react';
import { Link, useParams } from 'react-router-dom';
import { ApiError } from '../api/client';
import type { GraphPayload, ObservedEventDto } from '../api/types';
import { MetaStrip } from '../components/MetaStrip';
import { SourcePanel } from '../components/SourcePanel';
import { Waterfall } from '../components/replay/Waterfall';
import { TopBar } from '../components/layout/TopBar';
import { KpiStrip } from '../components/replay/KpiStrip';
import { EpisodeStrip } from '../components/replay/EpisodeStrip';
import { WhyPanel } from '../components/replay/WhyPanel';
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
  useFindingEvidenceQuery,
} from '../lib/queries';
import { useSessionWindow } from '../hooks/useSessionWindow';
import type { LiveEnvelope } from '../hooks/useLiveStream';
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

  // --- WhyPanel inputs ---
  const selectedFinding = useMemo(() => {
    if (!sel.selectedFindingId) return null;
    return findingsData.find((f) => f.finding_id === sel.selectedFindingId) ?? null;
  }, [sel.selectedFindingId, findingsData]);

  const evidenceQuery = useFindingEvidenceQuery(sel.selectedFindingId ?? '', {
    enabled: !!sel.selectedFindingId && sel.whyPanelOpen,
  });

  // --- render branches ---
  const detailError = detail.error as ApiError | null;
  const is404 = detailError instanceof ApiError && detailError.status === 404;
  const isLoading = detail.isLoading;
  const effectiveGraph = graph.data ?? EMPTY_GRAPH;

  const selectedNode =
    sel.selectedNodeId && effectiveGraph
      ? effectiveGraph.nodes.find((n) => n.node_id === sel.selectedNodeId) ?? null
      : null;
  const selectedEventId = selectedNode?.source_event_ids[0] ?? null;

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
            {/* Sentinel parked at the top of the stream slot: R2's ConversationStream is the scrollable list, and scrolling up to here triggers loadOlder. Until R2 lands the slot holds only the placeholder below. */}
            <div
              ref={sentinelRef}
              aria-hidden
              style={{ height: 1 }}
              data-testid="scroll-sentinel"
            />
            {/* R2 replaces this slot with ConversationStream. */}
            <p className={styles.placeholder}>Conversation stream (R2)</p>
          </div>

          <div className={styles.detail} data-slot="detail">
            {/* R3 replaces this slot with the tabbed DetailPanel. */}
            <SourcePanel eventId={selectedEventId} node={selectedNode} />
          </div>

          <div className={styles.timeline} data-slot="timeline">
            <Waterfall
              graph={effectiveGraph}
              selectedNodeId={sel.selectedNodeId}
              onSelect={(id) => sel.setSelectedNodeId(id)}
            />
          </div>
        </div>
      )}

      {!isLoading && !is404 && !detail.data && detail.isError && (
        <p role="alert">{detail.error?.message ?? 'failed'}</p>
      )}

      <WhyPanel
        open={sel.whyPanelOpen}
        finding={selectedFinding}
        evidence={evidenceQuery.data}
        onClose={sel.closeWhyPanel}
        onEvidenceHover={sel.setHoveredNodeId}
      />
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
