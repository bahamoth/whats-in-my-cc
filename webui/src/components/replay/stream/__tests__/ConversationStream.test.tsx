// webui/src/components/replay/stream/__tests__/ConversationStream.test.tsx
/**
 * ConversationStream renders a StreamItem[] (MessageCard for messages, one
 * ActivityStack per activity-run), oldest→newest (newest at the DOM bottom),
 * forwards clicks, reflects selection, and preserves virtualization /
 * scroll-into-view / autoscroll. Spec §3.
 */
import { render, screen, fireEvent } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { ConversationStream } from '../ConversationStream';
import type { MessageItem, ActivityRun, ActivityEvent } from '../streamModel';

// Capture the options ConversationStream passes to useVirtualizer so we can
// assert it keys the row-size cache by stable item id (see overlap regression
// test below). The wrapper calls through, so all other tests are unaffected.
const hoisted = vi.hoisted(() => ({ captured: { opts: null as any } }));
vi.mock('@tanstack/react-virtual', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@tanstack/react-virtual')>();
  return {
    ...actual,
    useVirtualizer: (opts: any) => {
      hoisted.captured.opts = opts;
      return (actual as any).useVirtualizer(opts);
    },
  };
});

function msg(id: string, text: string, over: Partial<MessageItem> = {}): MessageItem {
  return {
    type: 'message',
    id,
    eventId: id,
    role: 'user',
    model: null,
    text,
    timestamp: '2026-05-28T09:14:02Z',
    sidechain: false,
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

describe('ConversationStream', () => {
  // Regression: rows overlapped because the virtualizer cached measured row
  // heights by INDEX, but useSessionWindow.loadOlder PREPENDS pages — shifting
  // every index — which desynced the size cache from the items (a tall row got
  // a short cached size → next row's offset landed inside it). Keying the cache
  // by stable item id makes prepend/reorder remap correctly. jsdom has no
  // layout (virtual path never runs), so we lock the contract here.
  it('keys the virtualizer size-cache by stable item id (prepend overlap regression)', () => {
    hoisted.captured.opts = null;
    const items = [msg('e1', 'a'), run('r1', [ae('c1', 'Read')]), msg('e2', 'b', { role: 'assistant' })];
    render(
      <ConversationStream
        items={items}
        selectedEventId={null}
        findingEventIds={new Set()}
        onSelect={() => {}}
      />,
    );
    const opts = hoisted.captured.opts;
    expect(opts).not.toBeNull();
    expect(typeof opts.getItemKey).toBe('function');
    expect(opts.getItemKey(0)).toBe('e1');
    expect(opts.getItemKey(1)).toBe('r1');
    expect(opts.getItemKey(2)).toBe('e2');
  });

  it('renders messages and activity stacks in source order', () => {
    render(
      <ConversationStream
        items={[
          msg('a', 'first'),
          run('r1', [ae('c1', 'Read'), ae('c2', 'Bash')]),
          msg('b', 'second', { role: 'assistant' }),
        ]}
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
        selectedEventId={null}
        findingEventIds={new Set()}
        onSelect={() => {}}
      />,
    );
    rerender(
      <ConversationStream
        items={items}
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
        selectedEventId={null}
        findingEventIds={new Set()}
        onSelect={() => {}}
      />,
    );
    const scroller = container.querySelector('[data-testid="conversation-stream"]') as HTMLElement;
    Object.defineProperty(scroller, 'scrollHeight', { value: 1000, configurable: true });
    Object.defineProperty(scroller, 'clientHeight', { value: 300, configurable: true });
    // A genuine user gesture (wheel) marks the ensuing scroll as user-driven, so
    // it disengages stick-to-bottom. (Measurement-only scrolls are ignored.)
    fireEvent.wheel(scroller, { deltaY: -120 });
    scroller.scrollTop = 100;
    fireEvent.scroll(scroller);
    rerender(
      <ConversationStream
        items={[...items, msg('z', 'third')]}
        selectedEventId={null}
        findingEventIds={new Set()}
        onSelect={() => {}}
      />,
    );
    expect(scroller.scrollTop).toBe(100);
  });

  // Regression lock for the "can't focus while streaming" bug: after the user
  // scrolls up, a later MEASUREMENT-driven scroll event (no user gesture, fired
  // by the virtualizer as it measures rows) must NOT re-engage stick-to-bottom,
  // even if it momentarily reports the viewport at the bottom.
  it('ignores measurement-driven scroll events when deciding stick-to-bottom', () => {
    const nowSpy = vi.spyOn(performance, 'now');
    const items = [msg('a', 'first'), msg('b', 'second')];
    const { container, rerender } = render(
      <ConversationStream items={items} selectedEventId={null} findingEventIds={new Set()} onSelect={() => {}} />,
    );
    const scroller = container.querySelector('[data-testid="conversation-stream"]') as HTMLElement;
    Object.defineProperty(scroller, 'scrollHeight', { value: 1000, configurable: true });
    Object.defineProperty(scroller, 'clientHeight', { value: 300, configurable: true });

    // t=1000ms: user scrolls up via wheel → stick disengages.
    nowSpy.mockReturnValue(1000);
    fireEvent.wheel(scroller, { deltaY: -120 });
    scroller.scrollTop = 100;
    fireEvent.scroll(scroller);

    // t=2000ms (>200ms later): a measurement scroll reports the viewport at the
    // bottom. With no recent user gesture it must be ignored, so stick stays off.
    nowSpy.mockReturnValue(2000);
    scroller.scrollTop = 700; // dist = 1000-700-300 = 0 (looks "at bottom")
    fireEvent.scroll(scroller);

    rerender(
      <ConversationStream items={[...items, msg('z', 'third')]} selectedEventId={null} findingEventIds={new Set()} onSelect={() => {}} />,
    );
    // If the measurement scroll had wrongly re-stuck, append would pin to 1000.
    expect(scroller.scrollTop).toBe(700);
    nowSpy.mockRestore();
  });
});
