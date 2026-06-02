import { useEffect, useMemo, useRef } from 'react';
import { Link, useParams } from 'react-router-dom';
import { ApiError } from '../api/client';
import { MetaStrip } from '../components/MetaStrip';
import { DetailPanel } from '../components/replay/detail/DetailPanel';
import { TopBar } from '../components/layout/TopBar';
import { InsightStrip } from '../components/replay/insight-strip/InsightStrip';
import {
  ReplaySelectionProvider,
  useReplaySelection,
} from '../components/replay/selection/ReplaySelection';
import { useLiveStreamBridge } from '../lib/sse';
import {
  useSessionDetailQuery,
  useFindingsQuery,
  useVerificationRunsQuery,
  useToolFailureSummaryQuery,
  useSessionUsageQuery,
  useUsageBaselineQuery,
  useEventRawQuery,
} from '../lib/queries';
import { useSessionWindow } from '../hooks/useSessionWindow';
import { ConversationStream } from '../components/replay/stream/ConversationStream';
import { UntaggedBashPanel } from '../components/replay/stream/UntaggedBashPanel';
import { buildStreamModel } from '../components/replay/stream/streamModel';
import { buildLlmRequestMetrics } from '../components/replay/stream/llmRequestMetrics';
import type { RawBlock } from '../components/replay/detail/RawTab';
import { buildRawBlocksFromEvents } from '../components/replay/detail/rawBlocks';
import {
  buildToolMetricsFromEvents,
  buildLlmMetricsFromEvents,
} from '../components/replay/detail/eventMetrics';
import styles from './SessionDetailPage.module.css';

// Debounce window for SSE-driven backfill: an envelope burst collapses to one
// forward `?after=` page fetch (mirrors the bridge's graph-invalidate debounce).
const BACKFILL_DEBOUNCE_MS = 600;

function SessionDetailInner({ sessionId }: { sessionId: string }) {
  const sel = useReplaySelection();

  const detail = useSessionDetailQuery(sessionId);
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
    // Neither `gap` nor `resync` should wipe the loaded window. The window's
    // older pages come from REST (`?before=`) and are always authoritative;
    // the SSE stream only feeds the live tip. So on either signal we catch the
    // tip up with loadNewer and never reload — reloading discarded every older
    // page the reader had scrolled back to load and snapped the view to the
    // newest event ("loaded history disappears / focus jumps to bottom").
    //
    // Why both fire often here: the SSE broadcast channel is shared across ALL
    // sessions (src/api/sse.rs), so it lags (→ `gap`) whenever another session
    // is busy, and the connection drops/reconnects with a stale cursor the
    // backend can't backfill (→ `resync`) under the same load.
    onGap: () => void window_.loadNewer(),
    onResync: () => void window_.loadNewer(),
  });

  // Older history is paged by ConversationStream's own near-top scroll (see its
  // onLoadOlder). The previous IntersectionObserver sentinel lived in this
  // (non-scrolling) container and re-fired on every render, auto-loading the
  // whole session — so it has been removed.

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

  // event ids that have a finding. evidence_refs are event ids (bare-string
  // ULID or { event_id }) — resolved directly, no graph node mapping.
  const findingEventIds = useMemo(() => {
    const eids = new Set<string>();
    for (const f of findingsData) {
      for (const ref of f.evidence_refs) {
        if (typeof ref === 'string') eids.add(ref);
        else if (typeof ref.event_id === 'string') eids.add(ref.event_id);
      }
    }
    return eids;
  }, [findingsData]);

  // The views are event-first: selection IS the event id (no graph node).
  const selectedEventId = sel.selectedNodeId;
  const selectedStreamEventId = selectedEventId;
  const selectStreamCard = (eventId: string) => sel.setSelectedNodeId(eventId);

  // --- DetailPanel inputs (all event-derived; no graph) ---
  const selectedEvent = useMemo(
    () =>
      selectedEventId
        ? window_.events.find((e) => e.event_id === selectedEventId) ?? null
        : null,
    [selectedEventId, window_.events],
  );

  const rawQuery = useEventRawQuery(selectedEventId);

  // Findings for the selected event. evidence_refs are event ids (L1/L2
  // extractors emit event_id refs); bare-string and {event_id} refs both match.
  const selectedNodeFindings = useMemo(() => {
    if (!selectedEventId) return [];
    return findingsData.filter((f) =>
      f.evidence_refs.some((ref) =>
        typeof ref === 'string' ? ref === selectedEventId : ref.event_id === selectedEventId,
      ),
    );
  }, [selectedEventId, findingsData]);

  // Tool-execution metrics for a selected tool_call, found among the loaded
  // events by tool_use_id (no facet fold).
  const selectedToolMetrics = useMemo(
    () =>
      selectedEvent?.kind === 'tool_call'
        ? buildToolMetricsFromEvents(window_.events, selectedEvent.tool_use_id)
        : null,
    [selectedEvent, window_.events],
  );

  // Per-response metrics for a selected assistant_message / thinking, found by
  // request_id (llm_request span merged with api_request log cost).
  const selectedLlmMetrics = useMemo(
    () =>
      selectedEvent &&
      (selectedEvent.kind === 'assistant_message' || selectedEvent.kind === 'thinking')
        ? buildLlmMetricsFromEvents(window_.events, selectedEvent.request_id ?? null)
        : null,
    [selectedEvent, window_.events],
  );

  // Source-split raw blocks for the Raw tab, built from the selected event +
  // correlated events (matched tool_result, telemetry by tool_use_id/request_id).
  // Falls back to the single `record` (rawQuery) when there is nothing to split.
  const rawBlocks = useMemo<RawBlock[] | undefined>(
    () => (selectedEvent ? buildRawBlocksFromEvents(selectedEvent, window_.events) : undefined),
    [selectedEvent, window_.events],
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
            <MetaStrip session={detail.data} events={window_.events} />
          </div>

          <div className={styles.stream} data-slot="stream">
            {window_.loading === 'older' && (
              <div className={styles.loadingOlder} role="status" aria-live="polite">
                <span className={styles.spinner} aria-hidden />
                이전 메시지 불러오는 중…
              </div>
            )}
            <ConversationStream
              items={streamItems}
              selectedEventId={selectedStreamEventId}
              findingEventIds={findingEventIds}
              onSelect={selectStreamCard}
              onLoadOlder={window_.loadOlder}
              canLoadOlder={window_.oldest !== null}
            />
          </div>

          <div className={styles.detail} data-slot="detail">
            <DetailPanel
              event={selectedEvent}
              record={rawQuery.data?.record ?? null}
              findings={selectedNodeFindings}
              toolMetrics={selectedToolMetrics}
              llmMetrics={selectedLlmMetrics}
              rawBlocks={rawBlocks}
            />
          </div>

          <UntaggedBashPanel events={window_.events} />
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
