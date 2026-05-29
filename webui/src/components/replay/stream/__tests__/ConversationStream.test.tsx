// webui/src/components/replay/stream/__tests__/ConversationStream.test.tsx
/**
 * R2 RED — ConversationStream renders cards oldest→newest (newest at the
 * DOM bottom), forwards clicks, and reflects selection. Spec §3.
 */
import { render, screen, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { ConversationStream } from '../ConversationStream';
import type { StreamCard } from '../streamModel';

function c(id: string, preview: string): StreamCard {
  return { id, eventId: id, kind: 'user', actor: 'user', timestamp: '2026-05-28T09:14:02Z', preview, tool: null };
}

describe('ConversationStream', () => {
  it('renders one card per item in source order (newest last in the DOM)', () => {
    render(
      <ConversationStream
        cards={[c('a', 'first'), c('b', 'second')]}
        selectedEventId={null}
        phaseByEventId={{}}
        findingEventIds={new Set()}
        onSelect={() => {}}
      />,
    );
    const cards = screen.getAllByTestId('stream-card');
    expect(cards).toHaveLength(2);
    expect(within(cards[0]).getByText('first')).toBeInTheDocument();
    expect(within(cards[1]).getByText('second')).toBeInTheDocument();
  });

  it('marks the selected card', () => {
    render(
      <ConversationStream
        cards={[c('a', 'first'), c('b', 'second')]}
        selectedEventId="b"
        phaseByEventId={{}}
        findingEventIds={new Set()}
        onSelect={() => {}}
      />,
    );
    const cards = screen.getAllByTestId('stream-card');
    expect(cards[1].getAttribute('data-selected')).toBe('true');
    expect(cards[0].getAttribute('data-selected')).toBe('false');
  });

  it('passes the episode phase and finding marker through to the card', () => {
    render(
      <ConversationStream
        cards={[c('a', 'first')]}
        selectedEventId={null}
        phaseByEventId={{ a: 'repair' }}
        findingEventIds={new Set(['a'])}
        onSelect={() => {}}
      />,
    );
    expect(screen.getByText('repair')).toBeInTheDocument();
    expect(screen.getByLabelText(/finding/i)).toBeInTheDocument();
  });

  it('renders an empty hint when there are no cards', () => {
    render(
      <ConversationStream cards={[]} selectedEventId={null} phaseByEventId={{}} findingEventIds={new Set()} onSelect={() => {}} />,
    );
    expect(screen.getByText(/no conversation/i)).toBeInTheDocument();
  });

  it('forwards clicks with the event id', () => {
    const onSelect = vi.fn();
    render(
      <ConversationStream cards={[c('a', 'first')]} selectedEventId={null} phaseByEventId={{}} findingEventIds={new Set()} onSelect={onSelect} />,
    );
    screen.getByTestId('stream-card').click();
    expect(onSelect).toHaveBeenCalledWith('a');
  });
});
