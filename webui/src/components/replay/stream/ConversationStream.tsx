// webui/src/components/replay/stream/ConversationStream.tsx
import { useEffect, useLayoutEffect, useRef } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { MessageCard } from './MessageCard';
import { ActivityStack } from './ActivityStack';
import { SubagentGroup } from './SubagentGroup';
import { ThinkingMarker } from './ThinkingMarker';
import type { StreamItem } from './streamModel';
import { shouldLoadOlder } from './scrollAnchor';
import styles from './ConversationStream.module.css';

const FALLBACK_CAP = 200;

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
}

/** True when the item is a message with the given eventId, or an activity-run
 * containing an event with that id. Used for scroll-into-view targeting. */
function itemContainsEvent(item: StreamItem, eventId: string): boolean {
  if (item.type === 'message') return item.eventId === eventId;
  if (item.type === 'sidechain-group') return item.items.some((i) => itemContainsEvent(i, eventId));
  if (item.type === 'thinking') return item.events.some((e) => e.eventId === eventId);
  return item.events.some((ae) => ae.event.event_id === eventId);
}

export function ConversationStream({
  items,
  selectedEventId,
  onSelect,
  findingEventIds,
  onLoadOlder,
  canLoadOlder = false,
}: ConversationStreamProps) {
  const parentRef = useRef<HTMLDivElement | null>(null);

  const virtualizer = useVirtualizer({
    count: items.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 64,
    // Key the measured-size cache by stable item id. Without this the cache is
    // index-based, and loadOlder PREPENDS pages (shifting every index) → cached
    // heights map to the wrong items → rows overlap. Stable ids remap correctly.
    getItemKey: (index) => items[index]?.id ?? index,
    overscan: 8,
    // End-anchored chat/log scrolling (react-virtual native):
    //  - anchorTo 'end': when older pages are PREPENDED, the virtualizer
    //    captures the visible keyed item and re-anchors scroll so it stays in
    //    place — including as those above-viewport rows re-measure (which the
    //    hand-rolled distance-from-bottom anchoring could not survive).
    //  - followOnAppend: when the reader is parked at the tip, new appended
    //    events (SSE backfill) keep the viewport pinned to the newest row;
    //    when scrolled up, the position is left alone.
    // NOTE: this behaviour needs real layout, so it is verified by browser
    // smoke + the options-contract test, not by jsdom unit assertions.
    anchorTo: 'end',
    followOnAppend: true,
    // "At the tip" tolerance. Rows measure lazily (estimate 64 → real height),
    // so the viewport lands a couple of rows short of the true bottom right
    // after the initial scroll-to-end; a tolerance lets the end-anchor re-pin
    // converge to the exact bottom AND keeps live-append following when the
    // reader is parked near (not pixel-exact at) the tip.
    scrollEndThreshold: 160,
  });

  const virtualItems = virtualizer.getVirtualItems();
  // jsdom / zero-height container: the virtualizer yields no items. Render all
  // items so behavior is observable in tests and on first paint before measure.
  const useVirtual = virtualItems.length > 0;

  // Start at the newest event (the bottom): the window loads the NEWEST page,
  // so the reader should land at the live tip, not the top of the window. Rows
  // measure lazily (estimate 64 → real height), so a single scroll-to-end lands
  // a little short; we re-pin to the bottom on each measurement tick
  // (getTotalSize change) UNTIL the reader makes a genuine scroll gesture. From
  // then on native anchorTo:'end' + followOnAppend own the scroll (prepend
  // stability + live-tip follow). followInitRef is cleared in markUserScroll.
  const followInitRef = useRef(true);
  const totalSize = virtualizer.getTotalSize();
  useLayoutEffect(() => {
    if (!followInitRef.current || items.length === 0) return;
    const el = parentRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [totalSize, items.length]);

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
  const markUserScroll = () => {
    hasInteractedRef.current = true;
    followInitRef.current = false; // a genuine gesture ends the initial bottom-pin
  };
  const onScroll = () => {
    const el = parentRef.current;
    if (!el) return;
    const prevScrollTop = prevScrollTopRef.current;
    prevScrollTopRef.current = el.scrollTop;
    if (!onLoadOlder) return;
    if (
      shouldLoadOlder({
        scrollTop: el.scrollTop,
        prevScrollTop,
        hasInteracted: hasInteractedRef.current,
        canLoadOlder,
      })
    ) {
      onLoadOlder();
    }
  };

  // Scroll the selected item into view when selection changes from an external
  // source (e.g. subgraph click). Keyed on selectedEventId only so it does not
  // fire on every append. When the selected event lives inside an activity-run,
  // ActivityStack auto-expands on
  // its own (it receives selectedEventId), so the row is mounted by the time
  // the fallback querySelector runs.
  useEffect(() => {
    if (!selectedEventId) return;
    const idx = items.findIndex((it) => itemContainsEvent(it, selectedEventId));
    if (idx < 0) return;
    // Virtual path: scroll the virtualizer to that index — but ONLY when the
    // selected row is not already on screen. Clicking a row that is already
    // visible (in-stream selection) must not re-center it and yank the
    // viewport; scroll-into-view is for OFF-SCREEN (external timeline/subgraph)
    // selection only.
    const vItems = virtualizer.getVirtualItems();
    const alreadyVisible = vItems.some((vi) => vi.index === idx);
    if (vItems.length > 0 && !alreadyVisible) {
      virtualizer.scrollToIndex(idx, { align: 'center' });
    }
    // Fallback path (jsdom / zero-height): find the element by data-event-id
    // and call scrollIntoView. In a real browser this works; in jsdom the stub
    // is a no-op but the spy can observe the call. Only message rows carry
    // data-event-id; activity rows rely on the virtual scrollToIndex above.
    if (typeof parentRef.current?.querySelector === 'function') {
      const escapedId = typeof CSS !== 'undefined' && typeof CSS.escape === 'function'
        ? CSS.escape(selectedEventId)
        : selectedEventId.replace(/[^\w-]/g, '\\$&');
      const el = parentRef.current.querySelector(`[data-event-id="${escapedId}"]`);
      if (el && typeof (el as HTMLElement).scrollIntoView === 'function') {
        (el as HTMLElement).scrollIntoView({ block: 'nearest' });
      }
    }
  // Deliberately keyed on selectedEventId only — must fire on selection change, not on every append.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedEventId]);

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
                style={{ position: 'absolute', top: 0, left: 0, width: '100%', transform: `translateY(${vi.start}px)` }}
              >
                {renderItem(item)}
              </div>
            );
          })}
        </div>
      ) : (
        fallbackItems.map((item) => (
          <div key={item.id} {...(rowEventId(item) ? { 'data-event-id': rowEventId(item) } : {})}>
            {renderItem(item)}
          </div>
        ))
      )}
    </div>
  );
}
