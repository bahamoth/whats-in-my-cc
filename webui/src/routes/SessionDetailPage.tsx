import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Link, useParams, useSearchParams } from 'react-router-dom';
import { BarChart3, PanelTopOpen } from 'lucide-react';
import { ApiError } from '../api/client';
import { MetaStrip } from '../components/MetaStrip';
import { DetailPanel } from '../components/replay/detail/DetailPanel';
import { TopBar } from '../components/layout/TopBar';
import { InsightStrip } from '../components/replay/insight-strip/InsightStrip';
import { InstructionCard } from '../components/replay/InstructionCard';
import { AnalysisPanel } from '../components/replay/analysis/AnalysisPanel';
import {
  ReplaySelectionProvider,
  useReplaySelection,
} from '../components/replay/selection/ReplaySelection';
import { useLiveStreamBridge } from '../lib/sse';
import {
  useSessionDetailQuery,
  useSignalsQuery,
  useVerificationRunsQuery,
  useSessionUsageQuery,
  useUsageBaselineQuery,
  useSessionTurnsQuery,
  useSessionTasksQuery,
  useSessionsListQuery,
  useEventRawQuery,
  useCorrelatedEventsQuery,
  useSessionMetricsQuery,
} from '../lib/queries';
import { teammatesOf } from '../lib/teamGrouping';
import { TeamStrip } from '../components/replay/TeamStrip';
import { TeamLinkProvider } from '../components/replay/stream/TeamLinkContext';
import { useSessionWindow } from '../hooks/useSessionWindow';
import { ConversationStream } from '../components/replay/stream/ConversationStream';
import { UntaggedBashPanel } from '../components/replay/stream/UntaggedBashPanel';
import { FilterBar } from '../components/replay/stream/FilterBar';
import {
  EMPTY_FILTER,
  filterFromSearch,
  filterKey,
  filterToSearch,
  isFilterActive,
  jumpNeedsFilterClear,
  toEventFilterParams,
  type FilterState,
} from '../components/replay/stream/filterState';
import { buildStreamModel } from '../components/replay/stream/streamModel';
import { insertInstructionMarkers } from '../components/replay/stream/streamInstructionMarkers';
import { getSessionInstructions } from '../api/client';
import type { InstructionObservationDto } from '../api/types';
import { stepEventId, nextErrorEventId } from '../components/replay/stream/streamKeyboard';
import { StreamLegend } from '../components/replay/stream/StreamLegend';
import { buildLlmRequestMetrics } from '../components/replay/stream/llmRequestMetrics';
import type { RawBlock } from '../components/replay/detail/RawTab';
import { buildRawBlocksFromEvents } from '../components/replay/detail/rawBlocks';
import {
  buildToolMetricsFromEvents,
  buildLlmMetricsFromEvents,
} from '../components/replay/detail/eventMetrics';
import { useT } from '../i18n';
import styles from './SessionDetailPage.module.css';

// Debounce window for SSE-driven backfill: an envelope burst collapses to one
// forward `?after=` page fetch (mirrors the bridge's graph-invalidate debounce).
const BACKFILL_DEBOUNCE_MS = 600;

// Stream-legend dismissal flag (legacy key, kept so prior dismissals carry over).
const LEGEND_DISMISS_KEY = 'wimcc.streamLegend.dismissed';

function SessionDetailInner({ sessionId }: { sessionId: string }) {
  const t = useT();
  const sel = useReplaySelection();

  // Event filter (§1.4) — URL-backed (`f_*` params) so a filtered view is
  // deep-linkable/shareable. `applyFilter` round-trips through `filterToSearch`
  // onto the CURRENT search params (functional updater) so it never clobbers
  // other keys (`selected`, `finding`) a sibling hook (ReplaySelection) may be
  // writing in the same tick.
  const [searchParams, setSearchParams] = useSearchParams();
  const filter = useMemo(() => filterFromSearch(searchParams), [searchParams]);
  const filterActive = isFilterActive(filter);
  const applyFilter = useCallback(
    (f: FilterState) => {
      setSearchParams(
        (sp) => {
          const next = new URLSearchParams(sp);
          filterToSearch(f, next);
          return next;
        },
        { replace: true },
      );
    },
    [setSearchParams],
  );
  // Transient notice shown in the FilterBar when a jump (signal evidence, etc.)
  // forces the filter to clear because its target isn't in the filtered
  // buffer (§1.4) — auto-dismissed after 4s.
  const [jumpNotice, setJumpNotice] = useState<string | null>(null);

  const detail = useSessionDetailQuery(sessionId);
  const signals = useSignalsQuery(sessionId);
  // 팀 관계(리드↔팀메이트) 조인 데이터 — TeamStrip 배지와 teammate 응답
  // 카드의 세션 점프 링크가 공유한다 (2026-07-03).
  const sessionsList = useSessionsListQuery();
  const teammateSessionByName = useMemo(() => {
    const mates = teammatesOf(sessionsList.data ?? [], sessionId);
    return Object.fromEntries(
      mates.filter((m) => m.agent_name).map((m) => [m.agent_name as string, m.session_id]),
    );
  }, [sessionsList.data, sessionId]);
  const verificationRuns = useVerificationRunsQuery(sessionId);
  const usage = useSessionUsageQuery(sessionId);
  const baseline = useUsageBaselineQuery();
  const turns = useSessionTurnsQuery(sessionId);

  // Per-task summaries (status·duration·work-span aggregations), computed
  // server-side by the task_summary aggregator (GET /v1/sessions/:id/tasks).
  // buildStreamModel collects tasks into one inline TaskList block.
  const tasksQuery = useSessionTasksQuery(sessionId);

  // Analysis surface — separate from replay (spec §8.3, 원칙 7)
  const [analysisOpen, setAnalysisOpen] = useState(false);
  const metricsQuery = useSessionMetricsQuery(sessionId, { enabled: analysisOpen && !!sessionId });

  // Stream legend visibility is owned HERE (not inside StreamLegend) so that
  // dismissing it reclaims its full vertical space — the re-open control is a
  // toggle in the toolbar row below, costing zero extra layout. Persisted: once
  // dismissed it stays dismissed across mounts (legacy key reused).
  const [legendOpen, setLegendOpen] = useState(() => {
    try {
      return localStorage.getItem(LEGEND_DISMISS_KEY) !== '1';
    } catch {
      return true;
    }
  });
  const setLegend = useCallback((open: boolean) => {
    try {
      if (open) localStorage.removeItem(LEGEND_DISMISS_KEY);
      else localStorage.setItem(LEGEND_DISMISS_KEY, '1');
    } catch {
      /* ignore quota / private-mode failures — persistence is best-effort */
    }
    setLegendOpen(open);
  }, []);

  // Event id from a `?selected=` deep-link present AT MOUNT (captured once, so it
  // stays stable as the user later selects other events). Drives the initial
  // around-window + detached follow so a live-session deep-link actually lands.
  const [initialDeepLinkId] = useState(() => sel.selectedNodeId);
  const window_ = useSessionWindow(sessionId, {
    initialAround: initialDeepLinkId,
    filter: filterActive ? toEventFilterParams(filter) : null,
    filterKey: filterKey(filter),
  });

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

  // Live-tail vs reading-history. While the reader is following the live tip we
  // backfill SSE envelopes into the window as before. While they are scrolled up
  // reading older history (autoscroll OFF) we PAUSE backfill — appending +
  // trimming the window would drop the oldest rows they are reading and snap the
  // viewport (the "wait a few seconds → jumps to a random card" bug). We just
  // count the pending arrivals for the "N ↓" badge; on resume we reload() to the
  // live tip to catch up. ConversationStream reports the follow state up here.
  const reload = window_.reload;
  // If the page MOUNTED on a `?selected=` deep-link, start DETACHED: the initial
  // load is the around-window (above), so following the live tip would let the
  // first SSE backfill pull the window off it and the card never lands
  // (regressed deep-link on LIVE sessions; static ones were unaffected).
  const mountedOnDeepLink = initialDeepLinkId !== null;
  const followingRef = useRef(!mountedOnDeepLink);
  const [pendingNew, setPendingNew] = useState(0);
  const handleFollowingChange = useCallback(
    (following: boolean) => {
      const was = followingRef.current;
      followingRef.current = following;
      if (following && !was) {
        // resume → catch up to the live tip, clear the pending indicator.
        setPendingNew(0);
        void reload();
      }
    },
    [reload],
  );

  useLiveStreamBridge(sessionId, {
    onEnvelope: () => {
      if (!followingRef.current) {
        // paused: don't touch the window, just surface that new events arrived.
        setPendingNew((c) => c + 1);
        return;
      }
      if (backfillTimerRef.current !== null) return;
      backfillTimerRef.current = setTimeout(() => {
        backfillTimerRef.current = null;
        void window_.loadNewer();
      }, BACKFILL_DEBOUNCE_MS) as unknown as number;
    },
    // gap/resync only catch the tip up while following; while reading history
    // they would do the same window-disturbing append, so they are paused too
    // (the resume reload() catches everything up).
    onGap: () => {
      if (followingRef.current) void window_.loadNewer();
      else setPendingNew((c) => c + 1);
    },
    onResync: () => {
      if (followingRef.current) void window_.loadNewer();
      else setPendingNew((c) => c + 1);
    },
  });

  // Signals drive the stream highlight + DetailPanel cross-reference (below).
  const signalsData = signals.data ?? [];

  // Per-response (LLM request) metrics, joined to thinking events by
  // request_id — the marker shows duration+tokens, selecting shows the rest.
  const metricsByReq = useMemo(
    () => buildLlmRequestMetrics(window_.events),
    [window_.events],
  );

  // 지시문 변경 관측(B-12) — 마커 삽입용. 실패/구서버는 빈 배열(마커 없음).
  const [instructionObs, setInstructionObs] = useState<InstructionObservationDto[]>([]);
  useEffect(() => {
    let alive = true;
    setInstructionObs([]);
    getSessionInstructions(sessionId)
      .then((rows) => {
        if (alive) setInstructionObs(rows);
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [sessionId]);

  const streamItems = useMemo(() => {
    const base = buildStreamModel(window_.events, metricsByReq, tasksQuery.data ?? [], {
      flat: filterActive,
    });
    if (instructionObs.length === 0) return base;
    const timeByEvent = new Map(window_.events.map((e) => [e.event_id, e.observed_at]));
    return insertInstructionMarkers(base, instructionObs, (id) => timeByEvent.get(id));
  }, [window_.events, metricsByReq, tasksQuery.data, instructionObs, filterActive]);

  // event ids that have a signal. evidence_refs are event ids (bare-string
  // ULID or { event_id }) — resolved directly, no graph node mapping.
  const signalEventIds = useMemo(() => {
    const eids = new Set<string>();
    for (const s of signalsData) {
      for (const ref of s.evidence_refs) {
        if (typeof ref === 'string') eids.add(ref);
        else if (typeof ref.event_id === 'string') eids.add(ref.event_id);
      }
    }
    return eids;
  }, [signalsData]);

  // The views are event-first: selection IS the event id (no graph node).
  const selectedEventId = sel.selectedNodeId;
  const selectedStreamEventId = selectedEventId;
  const selectStreamCard = (eventId: string) => sel.setSelectedNodeId(eventId);

  // S10 (§7.4) — stream keyboard nav: j/k move the selection down/up the spine,
  // e jumps to the next error event. The existing scroll-into-view effect
  // (ConversationStream) brings the new selection into view. Ignored while
  // typing in a field or when a modifier is held (lets ⌘K etc. pass through).
  const setSelectedNodeId = sel.setSelectedNodeId;
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.metaKey || e.ctrlKey || e.altKey) return;
      const t = e.target as HTMLElement | null;
      const tag = t?.tagName;
      if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || t?.isContentEditable) return;
      let next: string | null = null;
      if (e.key === 'j') next = stepEventId(streamItems, selectedEventId, 'down');
      else if (e.key === 'k') next = stepEventId(streamItems, selectedEventId, 'up');
      else if (e.key === 'e') next = nextErrorEventId(streamItems, selectedEventId);
      else return;
      if (next) {
        e.preventDefault();
        setSelectedNodeId(next);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [streamItems, selectedEventId, setSelectedNodeId]);

  // Deep-link `?selected=<event_id>` outside the loaded window (the initial
  // load is the newest tail): fetch the window AROUND the event and replace
  // the buffer with it, so the card mounts and the DetailPanel opens
  // (#doc-audit-2026-06-10 backlog). One attempt per event id — a 404 (event
  // gone via retention / wrong session) must not retry forever.
  const triedAroundRef = useRef<string | null>(null);
  const windowLoading = window_.loading;
  const windowEvents = window_.events;
  const loadAround = window_.loadAround;
  useEffect(() => {
    if (!selectedEventId) return;
    // While FOLLOWING the live tip, do NOT drag the window back to the selected
    // event. Resuming follow (autoscroll toggle ON) reloads to the tail on
    // purpose — the reader chose "latest" over the deep-linked event. Without
    // this guard, loadTail removes the (early) selected event from the window
    // and this effect immediately loadAround()s back to it, so the jump-to-latest
    // never lands (it snaps back to the deep-link slice). Selection is preserved
    // in the DetailPanel; only the auto-scroll-back is suppressed.
    if (followingRef.current) return;
    if (windowLoading !== 'idle') return;
    const targetInBuffer = windowEvents.some((e) => e.event_id === selectedEventId);
    if (targetInBuffer) return; // already loaded — the ConversationStream scroll effect handles it
    // §1.4 jump rule: a filter-active buffer only ever holds MATCHING rows, so
    // "outside the buffer" while filtered is undecidable from the client alone
    // (may be filtered out, or just not yet paged in) — clear the filter first
    // so the deterministic tail/around fetch below can find it. Clearing
    // changes `filterKey`, which resets the useSessionWindow buffer; this same
    // effect re-runs on the next `windowEvents` change and (now unfiltered)
    // falls through to `loadAround`.
    if (jumpNeedsFilterClear(filterActive, targetInBuffer)) {
      applyFilter(EMPTY_FILTER);
      setJumpNotice(t('filter.cleared.byJump'));
      setTimeout(() => setJumpNotice(null), 4000);
      return;
    }
    if (triedAroundRef.current === selectedEventId) return;
    triedAroundRef.current = selectedEventId;
    void loadAround(selectedEventId);
  }, [selectedEventId, windowLoading, windowEvents, loadAround, filterActive, applyFilter, t]);

  // --- DetailPanel inputs (all event-derived; no graph) ---
  const selectedEvent = useMemo(
    () =>
      selectedEventId
        ? window_.events.find((e) => e.event_id === selectedEventId) ?? null
        : null,
    [selectedEventId, window_.events],
  );

  const rawQuery = useEventRawQuery(selectedEventId);

  // On-demand correlated telemetry: a tool_call's tool_result/decision logs by
  // tool_use_id, or an assistant turn's llm_request span + api_request log by
  // request_id — so detail metrics populate even when that telemetry fell
  // outside the loaded message window. Merged with the window for the builders.
  const selToolUseId = selectedEvent?.kind === 'tool_call' ? selectedEvent.tool_use_id : null;
  const selRequestId =
    selectedEvent && (selectedEvent.kind === 'assistant_message' || selectedEvent.kind === 'thinking')
      ? selectedEvent.request_id ?? null
      : null;
  const correlated = useCorrelatedEventsQuery(sessionId, selToolUseId, selRequestId);
  const metricEvents = useMemo(
    () => (correlated.data ? [...window_.events, ...correlated.data.events] : window_.events),
    [window_.events, correlated.data],
  );

  // Signals for the selected event. evidence_refs are event ids (L1
  // extractors emit event_id refs); bare-string and {event_id} refs both match.
  const selectedNodeSignals = useMemo(() => {
    if (!selectedEventId) return [];
    return signalsData.filter((s) =>
      s.evidence_refs.some((ref) =>
        typeof ref === 'string' ? ref === selectedEventId : ref.event_id === selectedEventId,
      ),
    );
  }, [selectedEventId, signalsData]);

  // Tool-execution metrics for a selected tool_call, found among the loaded
  // events by tool_use_id (no facet fold).
  const selectedToolMetrics = useMemo(
    () =>
      selectedEvent?.kind === 'tool_call'
        ? buildToolMetricsFromEvents(metricEvents, selectedEvent.tool_use_id)
        : null,
    [selectedEvent, metricEvents],
  );

  // Per-response metrics for a selected assistant_message / thinking, found by
  // request_id (llm_request span merged with api_request log cost).
  const selectedLlmMetrics = useMemo(
    () =>
      selectedEvent &&
      (selectedEvent.kind === 'assistant_message' || selectedEvent.kind === 'thinking')
        ? buildLlmMetricsFromEvents(metricEvents, selectedEvent.request_id ?? null)
        : null,
    [selectedEvent, metricEvents],
  );

  // Source-split raw blocks for the Raw tab, built from the selected event +
  // correlated events (matched tool_result, telemetry by tool_use_id/request_id).
  // Falls back to the single `record` (rawQuery) when there is nothing to split.
  const rawBlocks = useMemo<RawBlock[] | undefined>(
    () => (selectedEvent ? buildRawBlocksFromEvents(selectedEvent, window_.events) : undefined),
    [selectedEvent, window_.events],
  );

  // Matched tool_result event for WhatSection: when the selected event is a
  // tool_call with a tool_use_id, find the corresponding tool_result event in
  // the loaded/correlated events (same lookup used by rawBlocks). This is the
  // event itself (not just the metrics), so WhatSection can show the full output.
  const matchedToolResult = useMemo(
    () =>
      selectedEvent?.kind === 'tool_call' && selectedEvent.tool_use_id
        ? (metricEvents.find(
            (e) => e.kind === 'tool_result' && e.tool_use_id === selectedEvent.tool_use_id,
          ) ?? null)
        : null,
    [selectedEvent, metricEvents],
  );

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
        <div
          className={analysisOpen ? styles.gridWithAnalysis : styles.grid}
          data-wimcc-detail-grid
        >
          <div className={styles.kpi} data-slot="kpi">
            <InsightStrip
              usage={usage.data}
              verificationRuns={verificationRuns.data}
              signals={signals.data}
              baseline={
                baseline.data
                  ? {
                      cache_hit_ratio: baseline.data.cache_hit_ratio.median,
                      billed_tokens: baseline.data.billed_tokens.median,
                    }
                  : undefined
              }
              turns={turns.data?.turns}
            />
            <InstructionCard sessionId={sessionId} />
            <MetaStrip session={detail.data} events={window_.events} />
            <TeamStrip sessionId={sessionId} agentSetting={detail.data?.agent_setting} />
            <div className={styles.toolbar}>
              <button
                className={styles.toolBtn}
                aria-pressed={legendOpen}
                onClick={() => setLegend(!legendOpen)}
                title={t('stream.legend.aria')}
              >
                <PanelTopOpen size={13} aria-hidden />
                {t('stream.legend.show')}
              </button>
              <button
                className={styles.toolBtn}
                aria-pressed={analysisOpen}
                onClick={() => setAnalysisOpen((v) => !v)}
              >
                <BarChart3 size={13} aria-hidden />
                {t('detail.analysisToggle')}
              </button>
            </div>
          </div>

          {analysisOpen && (
            <div className={styles.analysis} data-slot="analysis">
              <AnalysisPanel
                metrics={metricsQuery.data ?? null}
                signals={signalsData}
                onSelectEvent={selectStreamCard}
                data-testid="analysis-panel"
              />
            </div>
          )}

          <div className={styles.stream} data-slot="stream">
            <FilterBar
              filter={filter}
              onChange={applyFilter}
              matchedCount={window_.matchedCount}
              notice={jumpNotice}
            />
            <StreamLegend open={legendOpen} onClose={() => setLegend(false)} />
            {window_.loading === 'older' && (
              <div className={styles.loadingOlder} role="status" aria-live="polite">
                <span className={styles.spinner} aria-hidden />
                {t('common.loadingEarlier')}
              </div>
            )}
            <TeamLinkProvider value={teammateSessionByName}>
              <ConversationStream
                items={streamItems}
                selectedEventId={selectedStreamEventId}
                findingEventIds={signalEventIds}
                onSelect={selectStreamCard}
                onLoadOlder={window_.loadOlder}
                canLoadOlder={window_.oldest !== null}
                onLoadNewer={window_.loadNewer}
                canLoadNewer={!window_.atLiveTip}
                onFollowingChange={handleFollowingChange}
                initialFollow={!mountedOnDeepLink}
                pendingNewCount={pendingNew}
                flatMode={filterActive}
                filterActive={filterActive}
                footerExtra={
                  <UntaggedBashPanel events={window_.events} onJump={selectStreamCard} />
                }
              />
            </TeamLinkProvider>
          </div>

          <div className={styles.detail} data-slot="detail">
            <DetailPanel
              event={selectedEvent}
              record={rawQuery.data?.record ?? null}
              signals={selectedNodeSignals}
              toolMetrics={selectedToolMetrics}
              llmMetrics={selectedLlmMetrics}
              rawBlocks={rawBlocks}
              matchedResult={matchedToolResult}
              onSelectEvent={selectStreamCard}
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
