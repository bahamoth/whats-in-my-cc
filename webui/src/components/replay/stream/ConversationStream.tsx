// webui/src/components/replay/stream/ConversationStream.tsx
import { useEffect, useLayoutEffect, useRef } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { StreamCard } from './StreamCard';
import type { StreamCard as Card } from './streamModel';
import styles from './ConversationStream.module.css';

const FALLBACK_CAP = 200;

interface ConversationStreamProps {
  cards: Card[];
  selectedEventId: string | null;
  phaseByEventId: Record<string, string>;
  findingEventIds: Set<string>;
  onSelect: (eventId: string) => void;
}

export function ConversationStream({ cards, selectedEventId, phaseByEventId, findingEventIds, onSelect }: ConversationStreamProps) {
  const parentRef = useRef<HTMLDivElement | null>(null);
  const atBottomRef = useRef(true);

  const virtualizer = useVirtualizer({
    count: cards.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 64,
    overscan: 8,
  });

  const virtualItems = virtualizer.getVirtualItems();
  // jsdom / zero-height container: the virtualizer yields no items. Render all
  // cards so behavior is observable in tests and on first paint before measure.
  const useVirtual = virtualItems.length > 0;

  // Track whether the user is pinned to the bottom so live appends autoscroll
  // only when they haven't scrolled up to read history.
  const onScroll = () => {
    const el = parentRef.current;
    if (!el) return;
    atBottomRef.current = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
  };

  useLayoutEffect(() => {
    const el = parentRef.current;
    if (el && atBottomRef.current) el.scrollTop = el.scrollHeight;
  }, [cards.length]);

  useEffect(() => {
    // keep virtualizer range fresh when the set grows
    if (atBottomRef.current && cards.length > 0) virtualizer.scrollToIndex(cards.length - 1);
  }, [cards.length, virtualizer]);

  // Scroll the selected card into view when selection changes from an external
  // source (e.g. timeline or subgraph click). Keyed on selectedEventId only so
  // it does not conflict with the bottom-autoscroll effect above.
  useEffect(() => {
    if (!selectedEventId) return;
    const idx = cards.findIndex((c) => c.eventId === selectedEventId);
    if (idx < 0) return;
    // Virtual path: scroll the virtualizer to that index
    if (virtualizer.getVirtualItems().length > 0) {
      virtualizer.scrollToIndex(idx, { align: 'center' });
    }
    // Fallback path (jsdom / zero-height): find the element by data-event-id
    // and call scrollIntoView. In a real browser this works; in jsdom the stub
    // is a no-op but the spy can observe the call.
    if (typeof parentRef.current?.querySelector === 'function') {
      const escapedId = typeof CSS !== 'undefined' && typeof CSS.escape === 'function'
        ? CSS.escape(selectedEventId)
        : selectedEventId.replace(/[^\w-]/g, '\\$&');
      const el = parentRef.current.querySelector(`[data-event-id="${escapedId}"]`);
      if (el && typeof (el as HTMLElement).scrollIntoView === 'function') {
        (el as HTMLElement).scrollIntoView({ block: 'nearest' });
      }
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedEventId]);

  if (cards.length === 0) {
    return <p className={styles.empty}>No conversation events yet.</p>;
  }

  const renderCard = (card: Card) => (
    <StreamCard
      key={card.id}
      card={card}
      selected={card.eventId === selectedEventId}
      episodePhase={phaseByEventId[card.eventId] ?? null}
      hasFinding={findingEventIds.has(card.eventId)}
      onSelect={onSelect}
    />
  );

  // Fallback path: cap to the last FALLBACK_CAP cards (newest at bottom).
  const fallbackCards = cards.length > FALLBACK_CAP ? cards.slice(-FALLBACK_CAP) : cards;
  const fallbackCapped = cards.length > FALLBACK_CAP;

  return (
    <div
      ref={parentRef}
      className={styles.scroll}
      onScroll={onScroll}
      data-testid="conversation-stream"
      {...(!useVirtual && fallbackCapped ? { 'data-fallback-capped': 'true' } : {})}
    >
      {useVirtual ? (
        <div style={{ height: virtualizer.getTotalSize(), position: 'relative' }}>
          {virtualItems.map((vi) => {
            const card = cards[vi.index];
            return (
              <div
                key={card.id}
                ref={virtualizer.measureElement}
                data-index={vi.index}
                data-event-id={card.eventId}
                style={{ position: 'absolute', top: 0, left: 0, width: '100%', transform: `translateY(${vi.start}px)` }}
              >
                {renderCard(card)}
              </div>
            );
          })}
        </div>
      ) : (
        fallbackCards.map((card) => (
          <div key={card.id} data-event-id={card.eventId}>
            {renderCard(card)}
          </div>
        ))
      )}
    </div>
  );
}
