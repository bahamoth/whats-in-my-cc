// webui/src/components/replay/stream/ConversationStream.tsx
import { useEffect, useLayoutEffect, useMemo, useRef } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { MessageCard } from './MessageCard';
import { ActivityStack } from './ActivityStack';
import { splitRunByPhase } from './activityGroup';
import type { StreamItem } from './streamModel';
import styles from './ConversationStream.module.css';

const FALLBACK_CAP = 200;
const STICK_THRESHOLD = 48;

interface ConversationStreamProps {
  items: StreamItem[];
  phaseOf: (eventId: string) => string | null;
  selectedEventId: string | null;
  onSelect: (eventId: string) => void;
  findingEventIds: Set<string>;
}

/** True when the item is a message with the given eventId, or an activity-run
 * containing an event with that id. Used for scroll-into-view targeting. */
function itemContainsEvent(item: StreamItem, eventId: string): boolean {
  if (item.type === 'message') return item.eventId === eventId;
  return item.events.some((ae) => ae.event.event_id === eventId);
}

export function ConversationStream({
  items,
  phaseOf,
  selectedEventId,
  onSelect,
  findingEventIds,
}: ConversationStreamProps) {
  const parentRef = useRef<HTMLDivElement | null>(null);
  const stickRef = useRef(true);

  const virtualizer = useVirtualizer({
    count: items.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 64,
    overscan: 8,
  });

  const virtualItems = virtualizer.getVirtualItems();
  // jsdom / zero-height container: the virtualizer yields no items. Render all
  // items so behavior is observable in tests and on first paint before measure.
  const useVirtual = virtualItems.length > 0;

  // "Stick to bottom" follows live appends ONLY while the reader is parked at
  // the tip. The hard part: the virtualizer fires synthetic scroll events as it
  // measures rows, and those must NOT flip the stick decision (that re-engaged
  // autoscroll and yanked the viewport — the "can't focus while streaming"
  // bug). So we only let a scroll that closely follows a genuine user gesture
  // (wheel / pointer / key) change `stickRef`; measurement-driven scrolls are
  // ignored.
  const lastUserScrollRef = useRef(0);
  const markUserScroll = () => {
    lastUserScrollRef.current = performance.now();
  };
  const onScroll = () => {
    const el = parentRef.current;
    if (!el) return;
    if (performance.now() - lastUserScrollRef.current > 200) return; // measurement/programmatic → ignore
    stickRef.current = el.scrollHeight - el.scrollTop - el.clientHeight < STICK_THRESHOLD;
  };

  // Single source of autoscroll: when new items arrive and the reader is stuck
  // to the tip, jump to the bottom via native scrollTop. We deliberately do NOT
  // also call virtualizer.scrollToIndex — driving the same scroll position from
  // two mechanisms fights the virtualizer's measurement pass and oscillates.
  useLayoutEffect(() => {
    const el = parentRef.current;
    if (el && stickRef.current) el.scrollTop = el.scrollHeight;
  }, [items.length]);

  // Scroll the selected item into view when selection changes from an external
  // source (e.g. timeline or subgraph click). Keyed on selectedEventId only so
  // it does not conflict with the bottom-autoscroll effect above. When the
  // selected event lives inside an activity-run, ActivityStack auto-expands on
  // its own (it receives selectedEventId), so the row is mounted by the time
  // the fallback querySelector runs.
  useEffect(() => {
    if (!selectedEventId) return;
    const idx = items.findIndex((it) => itemContainsEvent(it, selectedEventId));
    if (idx < 0) return;
    // Virtual path: scroll the virtualizer to that index
    if (virtualizer.getVirtualItems().length > 0) {
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

  // Memoize phase-split stacks per activity-run so splitRunByPhase isn't
  // recomputed for every unaffected row on each render.
  const stacksByRunId = useMemo(() => {
    const m = new Map<string, ReturnType<typeof splitRunByPhase>>();
    for (const it of items) {
      if (it.type === 'activity-run') m.set(it.id, splitRunByPhase(it.events, phaseOf));
    }
    return m;
  }, [items, phaseOf]);

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
    const stacks = stacksByRunId.get(item.id) ?? splitRunByPhase(item.events, phaseOf);
    return (
      <>
        {stacks.map((stack, i) => (
          <ActivityStack
            key={`${item.id}-${i}`}
            stack={stack}
            selectedEventId={selectedEventId}
            onSelect={onSelect}
          />
        ))}
      </>
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
