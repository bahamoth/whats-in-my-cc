// webui/src/components/replay/stream/__tests__/ConversationStream.test.tsx
/**
 * ConversationStream renders a StreamItem[] (MessageCard for messages,
 * ActivityStack(s) for activity-runs via splitRunByPhase), oldest→newest
 * (newest at the DOM bottom), forwards clicks, reflects selection, and
 * preserves virtualization / scroll-into-view / autoscroll. Spec §3.
 */
import { render, screen, fireEvent } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { ConversationStream } from '../ConversationStream';
import type { MessageItem, ActivityRun, ActivityEvent } from '../streamModel';

function msg(id: string, text: string, over: Partial<MessageItem> = {}): MessageItem {
  return {
    type: 'message',
    id,
    eventId: id,
    role: 'user',
    model: null,
    text,
    timestamp: '2026-05-28T09:14:02Z',
    ...over,
  };
}

function ae(eventId: string, toolName: string, isError = false): ActivityEvent {
  return {
    event: {
      event_id: eventId,
      raw_event_id: '',
      session_id: 's1',
      event_uuid: null,
      parent_uuid: null,
      observed_at: '2026-05-28T09:14:02Z',
      actor: 'assistant',
      kind: 'tool_call',
      subkind: null,
      tool_use_id: eventId,
      tool_name: toolName,
      turn_id: null,
      is_sidechain: false,
      is_meta: false,
      payload: { tool_name: toolName, input: {} },
    },
    result: { isError },
  };
}

function run(id: string, events: ActivityEvent[]): ActivityRun {
  return { type: 'activity-run', id, events };
}

const noPhase = () => null;

describe('ConversationStream', () => {
  it('renders messages and activity stacks in source order', () => {
    render(
      <ConversationStream
        items={[
          msg('a', 'first'),
          run('r1', [ae('c1', 'Read'), ae('c2', 'Bash')]),
          msg('b', 'second', { role: 'assistant' }),
        ]}
        phaseOf={noPhase}
        selectedEventId={null}
        findingEventIds={new Set()}
        onSelect={() => {}}
      />,
    );
    expect(screen.getAllByTestId('message-card')).toHaveLength(2);
    expect(screen.getAllByTestId('activity-stack')).toHaveLength(1);
    expect(screen.getByText('first')).toBeInTheDocument();
    expect(screen.getByText('second')).toBeInTheDocument();
  });

  it('marks the selected message', () => {
    render(
      <ConversationStream
        items={[msg('a', 'first'), msg('b', 'second')]}
        phaseOf={noPhase}
        selectedEventId="b"
        findingEventIds={new Set()}
        onSelect={() => {}}
      />,
    );
    const cards = screen.getAllByTestId('message-card');
    expect(cards[1].getAttribute('data-selected')).toBe('true');
    expect(cards[0].getAttribute('data-selected')).toBe('false');
  });

  it('shows a finding marker for messages whose eventId is a finding', () => {
    render(
      <ConversationStream
        items={[msg('a', 'first')]}
        phaseOf={noPhase}
        selectedEventId={null}
        findingEventIds={new Set(['a'])}
        onSelect={() => {}}
      />,
    );
    expect(screen.getByLabelText(/finding/i)).toBeInTheDocument();
  });

  it('renders an empty hint when there are no items', () => {
    render(
      <ConversationStream
        items={[]}
        phaseOf={noPhase}
        selectedEventId={null}
        findingEventIds={new Set()}
        onSelect={() => {}}
      />,
    );
    expect(screen.getByText(/no conversation/i)).toBeInTheDocument();
  });

  it('forwards clicks from a message card with the event id', () => {
    const onSelect = vi.fn();
    render(
      <ConversationStream
        items={[msg('a', 'first')]}
        phaseOf={noPhase}
        selectedEventId={null}
        findingEventIds={new Set()}
        onSelect={onSelect}
      />,
    );
    screen.getByTestId('message-card').click();
    expect(onSelect).toHaveBeenCalledWith('a');
  });

  it('scrolls the selected message into view when selectedEventId changes', () => {
    const spy = vi.spyOn(Element.prototype, 'scrollIntoView').mockImplementation(() => {});
    const items = [msg('a', 'first'), msg('b', 'second'), msg('z', 'last')];
    const { container, rerender } = render(
      <ConversationStream
        items={items}
        phaseOf={noPhase}
        selectedEventId={null}
        findingEventIds={new Set()}
        onSelect={() => {}}
      />,
    );
    rerender(
      <ConversationStream
        items={items}
        phaseOf={noPhase}
        selectedEventId="b"
        findingEventIds={new Set()}
        onSelect={() => {}}
      />,
    );
    const target = container.querySelector('[data-event-id="b"]');
    expect(spy.mock.instances).toContain(target);
    spy.mockRestore();
  });

  it('does not scroll when selectedEventId is null', () => {
    const spy = vi.spyOn(Element.prototype, 'scrollIntoView').mockImplementation(() => {});
    render(
      <ConversationStream
        items={[msg('a', 'first')]}
        phaseOf={noPhase}
        selectedEventId={null}
        findingEventIds={new Set()}
        onSelect={() => {}}
      />,
    );
    expect(spy).not.toHaveBeenCalled();
    spy.mockRestore();
  });

  it('mounts a bounded number of items for a large input (does not render all 2000)', () => {
    const many = Array.from({ length: 2000 }, (_, i) => msg(`n${i}`, `msg ${i}`));
    const { container } = render(
      <ConversationStream
        items={many}
        phaseOf={noPhase}
        selectedEventId={null}
        findingEventIds={new Set()}
        onSelect={() => {}}
      />,
    );
    const mounted = screen.getAllByTestId('message-card').length;
    expect(mounted).toBeLessThanOrEqual(200);
    expect(
      container.querySelector('[data-testid="conversation-stream"]')?.getAttribute('data-fallback-capped'),
    ).toBe('true');
  });

  it('autoscrolls to the bottom when an item is appended and the user is at the tip', () => {
    const items = [msg('a', 'first'), msg('b', 'second')];
    const { container, rerender } = render(
      <ConversationStream
        items={items}
        phaseOf={noPhase}
        selectedEventId={null}
        findingEventIds={new Set()}
        onSelect={() => {}}
      />,
    );
    const scroller = container.querySelector('[data-testid="conversation-stream"]') as HTMLElement;
    Object.defineProperty(scroller, 'scrollHeight', { value: 1000, configurable: true });
    Object.defineProperty(scroller, 'clientHeight', { value: 300, configurable: true });
    rerender(
      <ConversationStream
        items={[...items, msg('z', 'third')]}
        phaseOf={noPhase}
        selectedEventId={null}
        findingEventIds={new Set()}
        onSelect={() => {}}
      />,
    );
    expect(scroller.scrollTop).toBe(1000);
  });

  it('does NOT autoscroll on append when the user has scrolled up', () => {
    const items = [msg('a', 'first'), msg('b', 'second')];
    const { container, rerender } = render(
      <ConversationStream
        items={items}
        phaseOf={noPhase}
        selectedEventId={null}
        findingEventIds={new Set()}
        onSelect={() => {}}
      />,
    );
    const scroller = container.querySelector('[data-testid="conversation-stream"]') as HTMLElement;
    Object.defineProperty(scroller, 'scrollHeight', { value: 1000, configurable: true });
    Object.defineProperty(scroller, 'clientHeight', { value: 300, configurable: true });
    scroller.scrollTop = 100;
    fireEvent.scroll(scroller);
    rerender(
      <ConversationStream
        items={[...items, msg('z', 'third')]}
        phaseOf={noPhase}
        selectedEventId={null}
        findingEventIds={new Set()}
        onSelect={() => {}}
      />,
    );
    expect(scroller.scrollTop).toBe(100);
  });
});
