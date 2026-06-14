// webui/src/components/replay/stream/ConversationStream.tsx
import { useEffect, useLayoutEffect, useMemo, useRef } from 'react';
import type { ReactNode } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { MessageCard } from './MessageCard';
import { ActivityStack } from './ActivityStack';
import { SubagentGroup } from './SubagentGroup';
import { BatchGroup } from './BatchGroup';
import { WorkflowGroup } from './WorkflowGroup';
import { ScaffoldGroup } from './ScaffoldGroup';
import { ThinkingMarker } from './ThinkingMarker';
import { AutoscrollToggle } from './AutoscrollToggle';
import { BgGutter } from './BgGutter';
import { computeBgGutter } from './streamModel';
import type { StreamItem } from './streamModel';
import { shouldLoadOlder, shouldAdjustOnItemResize, LOAD_OLDER_TOP_PX } from './scrollAnchor';
import { useAutoscroll } from '../../../hooks/useAutoscroll';
import styles from './ConversationStream.module.css';

const FALLBACK_CAP = 200;
// Fixed height of the "대화 시작" marker. Rendered in normal flow above the
// virtualized list; the virtualizer is told about it via `scrollMargin` so row
// offsets stay correct. Must match the marker's CSS height exactly.
const START_MARKER_PX = 40;

interface ConversationStreamProps {
  items: StreamItem[];
  selectedEventId: string | null;
  onSelect: (eventId: string) => void;
  findingEventIds: Set<string>;
  /** Page in the next older window. Called when the reader scrolls near the top
   *  with a genuine gesture and `canLoadOlder` is true. */
  onLoadOlder?: () => void;
  /** Whether older history remains to be paged in (drives the near-top trigger
   *  and stops it once the session start is reached). */
  canLoadOlder?: boolean;
  /** Reports the follow state (true = following the live tip, false = reading
   *  history) so the page can pause SSE backfill while detached and catch up on
   *  resume. Fired on every change. */
  onFollowingChange?: (following: boolean) => void;
  /** Count of live arrivals received while detached (the page counts them since
   *  backfill is paused). Shown as the "N ↓" badge on the autoscroll toggle. */
  pendingNewCount?: number;
  /** Extra content for the LEFT of the stream footer (e.g. the untagged-Bash
   *  control), so footer affordances are consistent in one bar. */
  footerExtra?: ReactNode;
}

/** True when the item is a message with the given eventId, or an activity-run
 * containing an event with that id. Used for scroll-into-view targeting. */
function itemContainsEvent(item: StreamItem, eventId: string): boolean {
  if (item.type === 'message') return item.eventId === eventId;
  if (item.type === 'sidechain-group') return item.items.some((i) => itemContainsEvent(i, eventId));
  if (item.type === 'batch-group')
    return item.agentGroups.some((g) => itemContainsEvent(g, eventId));
  if (item.type === 'scaffold-group') return item.items.some((i) => i.eventId === eventId);
  if (item.type === 'thinking') return item.events.some((e) => e.eventId === eventId);
  if (item.type === 'workflow-group')
    return item.agentGroups.some((g) => itemContainsEvent(g, eventId));
  return item.events.some((ae) => ae.event.event_id === eventId);
}

export function ConversationStream({
  items,
  selectedEventId,
  onSelect,
  findingEventIds,
  onLoadOlder,
  canLoadOlder = false,
  onFollowingChange,
  pendingNewCount,
  footerExtra,
}: ConversationStreamProps) {
  const parentRef = useRef<HTMLDivElement | null>(null);

  // Deep-link scroll-to-index coordination. While we scroll the virtualizer to a
  // freshly-selected OFF-SCREEN event, `scrollPendingRef` is true so the
  // measurement-resize compensation stands down — otherwise it adjusts scrollTop
  // mid-reconcile and cancels the core's scrollToIndex, so the view briefly hits
  // the event then drifts to a meaningless area. Cleared once the target row
  // renders or the reader takes over. `scrollAttemptsRef` caps settle frames so a
  // never-converging reconcile cannot suppress compensation forever.
  const scrollPendingRef = useRef(false);
  const scrollAttemptsRef = useRef(0);

  // "대화 시작" marker sits in normal flow above the virtualized list once the
  // session start is loaded. The virtualizer is told about that offset via
  // `scrollMargin` so row positions stay correct (rows subtract it below).
  const showStartMarker = canLoadOlder === false && items.length > 0;
  const scrollMargin = showStartMarker ? START_MARKER_PX : 0;

  const virtualizer = useVirtualizer({
    count: items.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 64,
    // Key the measured-size cache by stable item id. Without this the cache is
    // index-based, and loadOlder PREPENDS pages (shifting every index) → cached
    // heights map to the wrong items → rows overlap. Stable ids remap correctly.
    getItemKey: (index) => items[index]?.id ?? index,
    overscan: 8,
    // NOTE: react-virtual's `anchorTo:'end'` is intentionally NOT used. It fails
    // to hold position when a prepend lands at scrollTop≈0 (the common case
    // during a fast upward scroll) — the viewport jumps to the new top, so the
    // reader cannot keep paging older history without a scroll-down-then-up
    // dance. Prepend anchoring is done manually below (scrollHeight-delta), the
    // standard chat-log technique; live-tip following is owned by useAutoscroll.
    scrollMargin,
  });

  // Measurement-resize scroll compensation: geometric rule (entirely-above
  // rows only) instead of the core default, whose scrollDirection guard races
  // with the ResizeObserver and eats upward wheel input at giant unmeasured
  // rows ("위로 스크롤이 멈춤", 2026-06-11). See shouldAdjustOnItemResize for
  // the captured evidence. This is a PUBLIC INSTANCE FIELD on the virtualizer
  // (not a constructor option — virtual-core only reads `this.should…`), so it
  // is assigned every render; `scrollAdjustments` is typed private but is the
  // exact frame the core's own default uses, hence the narrow cast.
  virtualizer.shouldAdjustScrollPositionOnItemSizeChange = (item, _delta, instance) => {
    // While a deep-link scroll-to-index is converging, stand down: the core's own
    // scrollToIndex reconcile owns scrollTop, and adjusting it here mid-reconcile
    // cancels that reconcile so the view drifts off the target.
    if (scrollPendingRef.current) return false;
    // getScrollOffset/scrollAdjustments are typed private in this core
    // version's d.ts but are the exact frame the core's own default predicate
    // reads — narrow structural cast instead of `any`.
    const v = instance as unknown as { getScrollOffset(): number; scrollAdjustments: number };
    return shouldAdjustOnItemResize({
      itemEnd: item.end,
      scrollOffset: v.getScrollOffset(),
      scrollAdjustments: v.scrollAdjustments,
    });
  };

  const virtualItems = virtualizer.getVirtualItems();
  // jsdom / zero-height container: the virtualizer yields no items. Render all
  // items so behavior is observable in tests and on first paint before measure.
  const useVirtual = virtualItems.length > 0;

  // Explicit autoscroll (stick-to-bottom) controller — single owner of the
  // scroll-position policy. A lightweight signature lets it tell tip-appends
  // (follow / count) from prepends (ignore — anchorTo:'end' handles those).
  const signature = useMemo(
    () => ({
      first: items[0]?.id ?? null,
      last: items[items.length - 1]?.id ?? null,
      count: items.length,
    }),
    [items],
  );
  const auto = useAutoscroll(parentRef, signature);

  // Per-row background-subagent gutter cells (hairline lanes). Derived from the
  // standalone sidechain-groups' spans; keyed by row id. Recomputed only when
  // the item list changes.
  const gutterByRow = useMemo(() => computeBgGutter(items), [items]);

  // Measurement-settle pin: while following (autoscroll ON), keep the viewport
  // glued to the measured bottom as rows lazily measure (estimate 64 → real
  // height) and grow `getTotalSize()`. This replaces the old 2s `followInitRef`
  // bottom-pin — but gated on the EXPLICIT autoscroll state, so it never yanks
  // the viewport while the reader is scrolled up (autoscroll OFF → no pin).
  const totalSize = virtualizer.getTotalSize();
  useLayoutEffect(() => {
    if (auto.autoscroll) auto.pinToBottom();
  }, [totalSize, auto.autoscroll, auto.pinToBottom]);

  // Report follow-state changes up to the page (pause/resume SSE backfill).
  // Kept in a ref so a changing callback identity does not re-fire the effect;
  // it must fire only when the follow state actually flips.
  const onFollowingChangeRef = useRef(onFollowingChange);
  onFollowingChangeRef.current = onFollowingChange;
  useEffect(() => {
    onFollowingChangeRef.current?.(auto.autoscroll);
  }, [auto.autoscroll]);

  // Paging OLDER history is driven by the stream's own scroll (the previous
  // IntersectionObserver sentinel lived in a non-scrolling container and
  // re-fired every render, auto-loading the whole session). We page the next
  // older window only when the reader — who has interacted at least once —
  // scrolls UP into the near-top zone. The interaction latch excludes the
  // initial bottom-pin and any pre-interaction programmatic scroll; the upward
  // direction excludes the native anchorTo:'end' re-anchor (which scrolls DOWN
  // after a prepend), so a load can never re-trigger itself into a cascade.
  const hasInteractedRef = useRef(false);
  const prevScrollTopRef = useRef(0);
  // scrollHeight captured the instant loadOlder is triggered, so the prepend
  // layout effect below can shift scrollTop by exactly the height the older
  // page added — keeping the reader's content fixed in place.
  const prependAnchorRef = useRef<number | null>(null);
  const markUserScroll = () => {
    hasInteractedRef.current = true;
  };
  const onScroll = () => {
    // 1) update autoscroll (follow / detach) from the new position.
    auto.onScroll();
    // 2) prefetch older history near the top.
    const el = parentRef.current;
    if (!el) return;
    const prevScrollTop = prevScrollTopRef.current;
    prevScrollTopRef.current = el.scrollTop;
    if (!onLoadOlder) return;
    // Prefetch a full viewport ahead (floor LOAD_OLDER_TOP_PX) so the next page
    // is prepended before the reader hits the absolute top — seamless upward
    // reading, no "scroll down then up to re-trigger" dance.
    const topThreshold = Math.max(LOAD_OLDER_TOP_PX, el.clientHeight);
    if (
      shouldLoadOlder({
        scrollTop: el.scrollTop,
        prevScrollTop,
        hasInteracted: hasInteractedRef.current,
        canLoadOlder,
        topThreshold,
      })
    ) {
      // capture the pre-prepend height so the layout effect can hold position
      prependAnchorRef.current = el.scrollHeight;
      onLoadOlder();
    }
  };

  // Manual prepend anchoring (standard chat-log technique): when an older page
  // is prepended (first item id changes, last stays), shift scrollTop down by
  // exactly the height that was added above the viewport, so the reader's
  // content stays put and they can keep scrolling up to page further — instead
  // of the viewport snapping to the new top (react-virtual's anchorTo:'end'
  // left scrollTop at 0 in that case).
  const prevAnchorSigRef = useRef<{ first: string | null; last: string | null } | null>(null);
  useLayoutEffect(() => {
    const el = parentRef.current;
    const prev = prevAnchorSigRef.current;
    prevAnchorSigRef.current = { first: signature.first, last: signature.last };
    if (!el || prev === null) return;
    const isPrepend = signature.first !== prev.first && signature.last === prev.last;
    if (isPrepend && prependAnchorRef.current !== null) {
      const delta = el.scrollHeight - prependAnchorRef.current;
      if (delta > 0) el.scrollTop += delta;
    }
    prependAnchorRef.current = null;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [signature.first, signature.last]);

  // Stuck-at-top recovery: a fast upward scroll can outrun a slow older-fetch so
  // the near-top trigger is dropped (loadOlder no-ops while a prior load is in
  // flight), and an under-anchored prepend can leave the reader pinned at the
  // ABSOLUTE top. There no further scroll event fires (you cannot scroll past
  // the top), so onScroll can never re-trigger and older history stops until a
  // manual scroll-down-then-up. Re-evaluate the trigger after each settle
  // (items / canLoadOlder change), reading the RESTING scrollTop — passing
  // prevScrollTop === scrollTop means only shouldLoadOlder's at-top branch can
  // fire here, so it pages ONLY when pinned at the very top. loadOlder no-ops
  // while a load is in flight and stops at the session start (canLoadOlder
  // false); a successful prepend anchor scrolls the reader DOWN off the top,
  // which ends the re-check — so this cannot cascade. Runs after the manual
  // anchor (useLayoutEffect above) so it sees the post-anchor position.
  useEffect(() => {
    const el = parentRef.current;
    if (!el || !onLoadOlder) return;
    if (
      shouldLoadOlder({
        scrollTop: el.scrollTop,
        prevScrollTop: el.scrollTop,
        hasInteracted: hasInteractedRef.current,
        canLoadOlder,
      })
    ) {
      prependAnchorRef.current = el.scrollHeight;
      onLoadOlder();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [items, canLoadOlder]);

  // Scroll the selected item into view when selection changes from an external
  // source (e.g. deep link, untagged-bash jump). The satisfied-ref makes this
  // one-shot per selection: appends re-run the effect (items dep) but never
  // re-scroll an already-satisfied selection. The deferred case matters for
  // deep links — `?selected=` is set at mount while the event arrives later
  // via the `?around=` window replacement, so the first run finds no row and
  // the retry on the items change performs the scroll. When the selected event
  // lives inside an activity-run, ActivityStack auto-expands on its own (it
  // receives selectedEventId), so the row is mounted by the time the fallback
  // querySelector runs.
  const scrollSatisfiedRef = useRef<string | null>(null);
  useEffect(() => {
    if (!selectedEventId) {
      scrollSatisfiedRef.current = null;
      scrollPendingRef.current = false;
      scrollAttemptsRef.current = 0;
      return;
    }
    if (scrollSatisfiedRef.current === selectedEventId) return;
    const idx = items.findIndex((it) => itemContainsEvent(it, selectedEventId));
    if (idx < 0) return; // not loaded yet — retried when items / totalSize change
    const vItems = virtualizer.getVirtualItems();
    if (vItems.length > 0) {
      const rendered = vItems.some((vi) => vi.index === idx);
      if (rendered) {
        // Target row is mounted: it was already on screen (in-stream click — must
        // not re-center / yank) or the core's scrollToIndex reconcile has landed.
        // Done — let the measurement compensation resume.
        scrollSatisfiedRef.current = selectedEventId;
        scrollPendingRef.current = false;
        scrollAttemptsRef.current = 0;
      } else if (hasInteractedRef.current || scrollAttemptsRef.current > 30) {
        // Reader took over, or the reconcile never converged — stop trying so we
        // neither fight the reader nor suppress compensation forever.
        scrollSatisfiedRef.current = selectedEventId;
        scrollPendingRef.current = false;
        scrollAttemptsRef.current = 0;
      } else if (!scrollPendingRef.current) {
        // OFF-SCREEN deep link `?selected=` / timeline-subgraph selection: stop
        // following the live tip and scroll to the event ONCE. `scrollPendingRef`
        // suppresses the measurement compensation so the core's dynamic-height
        // reconcile is not cancelled mid-flight (the "briefly there then drifts to
        // a meaningless area" bug). The effect re-runs on `totalSize` as rows
        // measure; we wait for the target row to render rather than re-issuing.
        auto.disable();
        scrollPendingRef.current = true;
        scrollAttemptsRef.current = 0;
        virtualizer.scrollToIndex(idx, { align: 'center' });
      } else {
        // Pending: rows between here and the target are still measuring, so the
        // first scrollToIndex landed on an ESTIMATE (64px/row) far from the real
        // (taller group-container) offset. Re-issue on each measurement settle so
        // we march toward the target — each scroll renders + measures the rows it
        // passes, growing totalSize, which re-runs this effect with a better
        // offset until the target row finally renders. Capped above.
        scrollAttemptsRef.current += 1;
        virtualizer.scrollToIndex(idx, { align: 'center' });
      }
      return;
    }
    // Fallback path (jsdom / zero-height, no virtual items): find the element by
    // data-event-id and call scrollIntoView. In a real browser the virtual path
    // above runs instead; in jsdom the stub is a no-op but the spy observes it.
    if (typeof parentRef.current?.querySelector === 'function') {
      const escapedId = typeof CSS !== 'undefined' && typeof CSS.escape === 'function'
        ? CSS.escape(selectedEventId)
        : selectedEventId.replace(/[^\w-]/g, '\\$&');
      const el = parentRef.current.querySelector(`[data-event-id="${escapedId}"]`);
      if (el && typeof (el as HTMLElement).scrollIntoView === 'function') {
        (el as HTMLElement).scrollIntoView({ block: 'nearest' });
      }
      scrollSatisfiedRef.current = selectedEventId;
    }
  // `totalSize` is in deps so the effect re-runs as rows lazily measure, letting
  // us detect when the reconcile has rendered the target row. The satisfied/
  // pending refs are the real guards.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedEventId, items, totalSize]);

  if (items.length === 0) {
    return <p className={styles.empty}>No conversation events yet.</p>;
  }

  const renderItem = (item: StreamItem) => {
    if (item.type === 'message') {
      return (
        <MessageCard
          item={item}
          selected={item.eventId === selectedEventId}
          onSelect={onSelect}
          hasFinding={findingEventIds.has(item.eventId)}
        />
      );
    }
    if (item.type === 'sidechain-group') {
      return (
        <SubagentGroup
          group={item}
          selectedEventId={selectedEventId}
          onSelect={onSelect}
          findingEventIds={findingEventIds}
        />
      );
    }
    if (item.type === 'thinking') {
      return (
        <ThinkingMarker
          marker={item}
          selectedEventId={selectedEventId}
          onSelect={onSelect}
        />
      );
    }
    if (item.type === 'batch-group') {
      return (
        <BatchGroup
          group={item}
          selectedEventId={selectedEventId}
          onSelect={onSelect}
          findingEventIds={findingEventIds}
        />
      );
    }
    if (item.type === 'workflow-group') {
      return (
        <WorkflowGroup
          group={item}
          selectedEventId={selectedEventId}
          onSelect={onSelect}
          findingEventIds={findingEventIds}
        />
      );
    }
    if (item.type === 'scaffold-group') {
      return (
        <ScaffoldGroup
          group={item}
          selectedEventId={selectedEventId}
          onSelect={onSelect}
          findingEventIds={findingEventIds}
        />
      );
    }
    // An activity-run renders as ONE contiguous ActivityStack (its events).
    return (
      <ActivityStack
        stack={{ events: item.events }}
        selectedEventId={selectedEventId}
        onSelect={onSelect}
      />
    );
  };

  // Message rows carry data-event-id for scroll-into-view + cross-sync tests.
  // Activity rows have multiple events, so they carry none and rely on
  // scrollToIndex (virtual) + ActivityStack auto-expand for visibility.
  const rowEventId = (item: StreamItem): string | undefined =>
    item.type === 'message' ? item.eventId : undefined;

  // Fallback path: cap to the last FALLBACK_CAP items (newest at bottom).
  const fallbackItems = items.length > FALLBACK_CAP ? items.slice(-FALLBACK_CAP) : items;
  const fallbackCapped = items.length > FALLBACK_CAP;

  return (
    <>
      <div
        ref={parentRef}
        className={styles.scroll}
        onScroll={onScroll}
        onWheel={markUserScroll}
        onPointerDown={markUserScroll}
        onKeyDown={markUserScroll}
        data-testid="conversation-stream"
        {...(!useVirtual && fallbackCapped ? { 'data-fallback-capped': 'true' } : {})}
      >
        {showStartMarker && (
          <div className={styles.startMarker} style={{ height: START_MARKER_PX }}>
            대화 시작
          </div>
        )}
        {useVirtual ? (
          <div style={{ height: virtualizer.getTotalSize(), position: 'relative' }}>
            {virtualItems.map((vi) => {
              const item = items[vi.index];
              return (
                <div
                  key={item.id}
                  ref={virtualizer.measureElement}
                  data-index={vi.index}
                  {...(rowEventId(item) ? { 'data-event-id': rowEventId(item) } : {})}
                  // subtract scrollMargin: vi.start includes it, but rows are
                  // positioned WITHIN the height container that already sits
                  // below the start marker.
                  style={{ position: 'absolute', top: 0, left: 0, width: '100%', transform: `translateY(${vi.start - scrollMargin}px)` }}
                >
                  <div className={styles.row}>
                    <BgGutter row={gutterByRow.get(item.id)} />
                    <div className={styles.rowBody}>{renderItem(item)}</div>
                  </div>
                </div>
              );
            })}
          </div>
        ) : (
          fallbackItems.map((item) => (
            <div key={item.id} {...(rowEventId(item) ? { 'data-event-id': rowEventId(item) } : {})}>
              <div className={styles.row}>
                <BgGutter row={gutterByRow.get(item.id)} />
                <div className={styles.rowBody}>{renderItem(item)}</div>
              </div>
            </div>
          ))
        )}
      </div>
      <AutoscrollToggle
        autoscroll={auto.autoscroll}
        // While detached, SSE backfill is paused (no appends), so the page-level
        // pending count is the source of truth for the "N ↓" badge; fall back to
        // the controller's own count when the page does not supply one.
        newCount={pendingNewCount ?? auto.newCount}
        onEnable={auto.enable}
        onDisable={auto.disable}
        leftSlot={footerExtra}
      />
    </>
  );
}
