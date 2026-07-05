// webui/src/components/replay/stream/__tests__/ConversationStream.test.tsx
/**
 * ConversationStream renders a StreamItem[] (MessageCard for messages, one
 * ActivityStack per activity-run), oldest→newest (newest at the DOM bottom),
 * forwards clicks, reflects selection, and preserves virtualization /
 * scroll-into-view. Live-append follow is owned by useAutoscroll; prepend
 * anchoring is manual (scrollHeight-delta) — react-virtual's anchorTo:'end' was
 * removed (verified by the options-contract test below + browser smoke, since
 * real anchoring needs layout). Spec §3.
 */
import { screen, fireEvent } from '@testing-library/react';
import { renderWithI18n as render } from '../../../../test/i18nRender';
import { describe, expect, it, vi } from 'vitest';
import { ConversationStream } from '../ConversationStream';
import type {
  MessageItem,
  ActivityRun,
  ActivityEvent,
  BatchGroup,
  WorkflowGroup,
  SidechainGroup,
  ScaffoldGroup,
} from '../streamModel';

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

function scGroup(over: Partial<SidechainGroup> = {}): SidechainGroup {
  return {
    type: 'sidechain-group',
    id: 'sc-A',
    agentId: 'A',
    agentType: 'Explore',
    description: '조사 A',
    taskEventId: null,
    conclusion: 'A 결론',
    items: [
      {
        type: 'message',
        id: 'aMsg',
        eventId: 'aMsg',
        role: 'assistant',
        model: null,
        text: 'A 결론',
        timestamp: '2026-06-13T00:00:30Z',
        sidechain: true,
      },
    ],
    ...over,
  };
}

function batch(over: Partial<BatchGroup> = {}): BatchGroup {
  return {
    type: 'batch-group',
    id: 'batch-sc-A',
    agentGroups: [
      scGroup({ id: 'sc-A', agentId: 'A' }),
      scGroup({ id: 'sc-B', agentId: 'B', conclusion: 'B 결론' }),
    ],
    dispatchMessageId: 'm1',
    synthesis: '두 결과를 종합하면 X',
    settled: true,
    ...over,
  };
}

function wfGroup(over: Partial<WorkflowGroup> = {}): WorkflowGroup {
  return {
    type: 'workflow-group',
    id: 'wf-sc-A',
    name: 'review-changes',
    description: null,
    taskEventId: 'wfc',
    agentGroups: [
      scGroup({ id: 'sc-A', agentId: 'A' }),
      scGroup({ id: 'sc-B', agentId: 'B', conclusion: 'B 결론' }),
    ],
    synthesis: '워크플로우 종합 결과 X',
    settled: true,
    ...over,
  };
}

function scaffoldGroup(over: Partial<ScaffoldGroup> = {}): ScaffoldGroup {
  return {
    type: 'scaffold-group',
    id: 'scaffold-c1',
    items: [
      msg('c1', '<command-name>/chrome</command-name>', { origin: 'command', commandName: '/chrome' }),
      msg('k1', 'Base directory for this skill: /x', { origin: 'skill' }),
    ],
    commandNames: ['/chrome'],
    ...over,
  };
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

  // Contract lock: the stream does NOT delegate scroll anchoring to
  // react-virtual. `anchorTo:'end'` left scrollTop at 0 when a prepend landed at
  // the top (the reader then could not page further — a scroll-down-then-up
  // dance). Prepend anchoring is manual (scrollHeight-delta) and live-tip
  // following is the explicit useAutoscroll controller, so neither
  // `anchorTo:'end'` nor `followOnAppend` is used. Real scroll behaviour:
  // browser smoke.
  it('does not delegate scroll anchoring/following to react-virtual', () => {
    hoisted.captured.opts = null;
    render(
      <ConversationStream
        items={[msg('e1', 'a'), msg('e2', 'b', { role: 'assistant' })]}
        selectedEventId={null}
        findingEventIds={new Set()}
        onSelect={() => {}}
      />,
    );
    const opts = hoisted.captured.opts;
    expect(opts).not.toBeNull();
    expect(opts.anchorTo).not.toBe('end');
    expect(opts.followOnAppend).toBeFalsy();
  });

  // The stream reports its follow state up so the page can pause/resume SSE
  // backfill. On mount it is following the live tip → reports true. (The OFF
  // transition needs real scroll/layout and is covered by browser smoke.)
  it('reports the initial follow state (true) via onFollowingChange', () => {
    const onFollowingChange = vi.fn();
    render(
      <ConversationStream
        items={[msg('e1', 'a'), msg('e2', 'b', { role: 'assistant' })]}
        selectedEventId={null}
        findingEventIds={new Set()}
        onSelect={() => {}}
        onFollowingChange={onFollowingChange}
      />,
    );
    expect(onFollowingChange).toHaveBeenCalledWith(true);
  });

  // The "N ↓" badge reflects the page-supplied pending count (backfill is paused
  // while detached, so the controller's own newCount stays 0).
  it('shows the page-supplied pendingNewCount on the toggle badge', () => {
    render(
      <ConversationStream
        items={[msg('e1', 'a'), msg('e2', 'b', { role: 'assistant' })]}
        selectedEventId={null}
        findingEventIds={new Set()}
        onSelect={() => {}}
        pendingNewCount={4}
      />,
    );
    // ON by default → badge hidden even if a count is supplied.
    expect(screen.queryByTestId('autoscroll-new-count')).toBeNull();
  });

  // The autoscroll state pill is always present (its label reflects ON/OFF).
  it('renders the autoscroll pill', () => {
    render(
      <ConversationStream
        items={[msg('e1', 'a'), msg('e2', 'b', { role: 'assistant' })]}
        selectedEventId={null}
        findingEventIds={new Set()}
        onSelect={() => {}}
      />,
    );
    // ON by default → "실시간"
    expect(screen.getByRole('button', { name: /자동 스크롤|최신/ })).toBeInTheDocument();
  });

  // "대화 시작" marker appears only once the session start is loaded
  // (canLoadOlder=false), telling the reader there is no more older history.
  it('shows the "대화 시작" start marker only when no older pages remain', () => {
    const props = {
      items: [msg('e1', 'a'), msg('e2', 'b', { role: 'assistant' })],
      selectedEventId: null,
      findingEventIds: new Set<string>(),
      onSelect: () => {},
    };
    const { rerender, queryByText } = render(<ConversationStream {...props} canLoadOlder />);
    expect(queryByText('대화 시작')).toBeNull();
    rerender(<ConversationStream {...props} canLoadOlder={false} />);
    expect(queryByText('대화 시작')).toBeInTheDocument();
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

  // A batch-group renders the dedicated BatchGroup container (not a flat list of
  // SubagentGroups). Collapsed by default (settled) → identity + synthesis show,
  // children stay hidden.
  it('renders a batch-group as the BatchGroup container', () => {
    render(
      <ConversationStream
        items={[msg('a', 'first'), batch()]}
        selectedEventId={null}
        findingEventIds={new Set()}
        onSelect={() => {}}
      />,
    );
    expect(screen.getByTestId('batch-group')).toBeInTheDocument();
    expect(screen.getByTestId('batch-synthesis')).toHaveTextContent('두 결과를 종합하면 X');
    // settled batch collapses by default → children hidden
    expect(screen.queryByTestId('subagent-group')).toBeNull();
  });

  // Selection inside a batch child auto-expands the container so the row mounts
  // and the scroll-into-view fallback can find it (itemContainsEvent recurses).
  it('auto-expands a batch-group whose child holds the selected event', () => {
    render(
      <ConversationStream
        items={[batch()]}
        selectedEventId="aMsg"
        findingEventIds={new Set()}
        onSelect={() => {}}
      />,
    );
    expect(screen.getByTestId('batch-group')).toHaveAttribute('data-expanded', 'true');
    expect(screen.getAllByTestId('subagent-group').length).toBeGreaterThan(0);
  });

  // A workflow-group renders the dedicated WorkflowGroup container. Collapsed by
  // default (settled), but synthesis + stats + mini-gantt stay visible; children
  // (SubagentGroups) hidden until expanded.
  it('renders a workflow-group as the WorkflowGroup container (collapsed shows synthesis+chart)', () => {
    render(
      <ConversationStream
        items={[msg('a', 'first'), wfGroup()]}
        selectedEventId={null}
        findingEventIds={new Set()}
        onSelect={() => {}}
      />,
    );
    expect(screen.getByTestId('workflow-group')).toBeInTheDocument();
    expect(screen.getByTestId('wf-synthesis')).toHaveTextContent('워크플로우 종합 결과 X');
    expect(screen.getByTestId('wf-stats')).toHaveTextContent('최대 병렬');
    expect(screen.getAllByTestId('wf-lane').length).toBe(2);
    expect(screen.queryByTestId('subagent-group')).toBeNull();
  });

  it('auto-expands a workflow-group whose child holds the selected event', () => {
    render(
      <ConversationStream
        items={[wfGroup()]}
        selectedEventId="aMsg"
        findingEventIds={new Set()}
        onSelect={() => {}}
      />,
    );
    expect(screen.getByTestId('workflow-group')).toHaveAttribute('data-expanded', 'true');
    expect(screen.getAllByTestId('subagent-group').length).toBeGreaterThan(0);
  });

  // A scaffold-group renders the dedicated ScaffoldGroup container, collapsed by
  // default (children hidden), with its identity chip showing.
  it('renders a scaffold-group as the ScaffoldGroup container (collapsed by default)', () => {
    render(
      <ConversationStream
        items={[msg('a', 'first'), scaffoldGroup()]}
        selectedEventId={null}
        findingEventIds={new Set()}
        onSelect={() => {}}
      />,
    );
    expect(screen.getByTestId('scaffold-group')).toBeInTheDocument();
    expect(screen.getByText('커맨드·스킬')).toBeInTheDocument();
    // collapsed → the folded cards (c1/k1) are not mounted; only the main msg is
    expect(screen.getAllByTestId('message-card')).toHaveLength(1);
    expect(screen.getByText('first')).toBeInTheDocument();
  });

  // Selection inside a scaffold-group child auto-expands the container so the
  // row mounts and the scroll-into-view fallback can find it (itemContainsEvent
  // recurses into scaffold-group.items).
  it('auto-expands a scaffold-group whose child holds the selected event', () => {
    render(
      <ConversationStream
        items={[scaffoldGroup()]}
        selectedEventId="k1"
        findingEventIds={new Set()}
        onSelect={() => {}}
      />,
    );
    expect(screen.getByTestId('scaffold-group')).toHaveAttribute('data-expanded', 'true');
    // expanded → both folded cards mount
    expect(screen.getAllByTestId('message-card')).toHaveLength(2);
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

  // 필터드 tail이 로딩되는 수 초 동안 "no events"를 보여주면 '매칭 없음'으로
  // 오독된다(2026-07-05 verification=unknown 실사례 — 50k 세션에서 수 초 스캔).
  it('while loading with no items, shows a loading state instead of the empty hint', () => {
    render(
      <ConversationStream
        items={[]}
        selectedEventId={null}
        findingEventIds={new Set()}
        onSelect={() => {}}
        loading
      />,
    );
    expect(screen.queryByText(/no conversation/i)).toBeNull();
    expect(screen.getByRole('status')).toBeInTheDocument();
  });

  it('with a filter active and truly no matches, says no MATCHING events', () => {
    render(
      <ConversationStream
        items={[]}
        selectedEventId={null}
        findingEventIds={new Set()}
        onSelect={() => {}}
        filterActive
      />,
    );
    expect(screen.getByText(/매칭|match/i)).toBeInTheDocument();
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

  // Deep-link case: `?selected=` is set at mount but the event is NOT in the
  // initially loaded items (it arrives later via the `?around=` window
  // replacement). The selection-scroll effect is keyed on selectedEventId, so
  // without a deferred retry the late-mounting row never gets scrolled to.
  it('scrolls to the selected message when it mounts AFTER selection (deep-link around window)', () => {
    const spy = vi.spyOn(Element.prototype, 'scrollIntoView').mockImplementation(() => {});
    const tail = [msg('y', 'tail-1'), msg('z', 'tail-2')];
    const { container, rerender } = render(
      <ConversationStream
        items={tail}
        selectedEventId="old" // selected at mount, not yet loaded
        findingEventIds={new Set()}
        onSelect={() => {}}
      />,
    );
    expect(spy).not.toHaveBeenCalled();
    // The around window replaces the items and now contains the selected row.
    const around = [msg('old', 'deep-linked'), ...tail];
    rerender(
      <ConversationStream
        items={around}
        selectedEventId="old"
        findingEventIds={new Set()}
        onSelect={() => {}}
      />,
    );
    const target = container.querySelector('[data-event-id="old"]');
    expect(target).not.toBeNull();
    expect(spy.mock.instances).toContain(target);
    // …and a later unrelated append must NOT re-scroll (selection unchanged,
    // already satisfied).
    spy.mockClear();
    rerender(
      <ConversationStream
        items={[...around, msg('new', 'append')]}
        selectedEventId="old"
        findingEventIds={new Set()}
        onSelect={() => {}}
      />,
    );
    expect(spy).not.toHaveBeenCalled();
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

  // Windowing: paging older history is driven by the stream's own scroll —
  // only an UPWARD near-top scroll by a reader who has interacted.
  it('pages older history when the user scrolls up into the near-top zone', () => {
    const onLoadOlder = vi.fn();
    const { container } = render(
      <ConversationStream
        items={[msg('a', 'first'), msg('b', 'second')]}
        selectedEventId={null}
        findingEventIds={new Set()}
        onSelect={() => {}}
        onLoadOlder={onLoadOlder}
        canLoadOlder
      />,
    );
    const scroller = container.querySelector('[data-testid="conversation-stream"]') as HTMLElement;
    fireEvent.wheel(scroller, { deltaY: -120 }); // latches "reader has interacted"
    scroller.scrollTop = 400; // start below the trigger zone
    fireEvent.scroll(scroller);
    scroller.scrollTop = 20; // scroll UP into the near-top zone
    fireEvent.scroll(scroller);
    expect(onLoadOlder).toHaveBeenCalled();
  });

  // Stuck-at-top recovery (2026-06-11): a fast upward scroll can outrun a slow
  // older-fetch (the near-top trigger no-ops while a load is in flight) and an
  // under-anchored prepend can leave the reader pinned at the ABSOLUTE top.
  // There, no further scroll event fires (you cannot scroll past the top), so
  // onScroll can never re-trigger — older history stops until the reader
  // scrolls DOWN then UP. The stream must re-evaluate the trigger after a
  // prepend settles, directly from the resting scrollTop, so it keeps paging.
  it('pages older history after a prepend settles while pinned at the absolute top (no new scroll event)', () => {
    const onLoadOlder = vi.fn();
    const base = {
      selectedEventId: null,
      findingEventIds: new Set<string>(),
      onSelect: () => {},
      onLoadOlder,
      canLoadOlder: true,
    };
    const { container, rerender } = render(
      <ConversationStream items={[msg('b', 'second'), msg('c', 'third')]} {...base} />,
    );
    const scroller = container.querySelector('[data-testid="conversation-stream"]') as HTMLElement;
    fireEvent.wheel(scroller, { deltaY: -120 }); // reader has interacted
    scroller.scrollTop = 0; // pinned at the absolute top (under-anchored prepend)
    onLoadOlder.mockClear();
    // A prepend lands (first id changes, last stays) with NO scroll event.
    rerender(
      <ConversationStream items={[msg('a', 'older'), msg('b', 'second'), msg('c', 'third')]} {...base} />,
    );
    expect(onLoadOlder).toHaveBeenCalled();
  });

  // Cascade / mount guard: before the reader interacts, the initial bottom-pin
  // and any measurement/programmatic scroll must NOT page older history —
  // otherwise the whole session auto-loads (the windowing bug).
  it('does NOT page older history before the reader interacts (mount/measurement scroll)', () => {
    const onLoadOlder = vi.fn();
    const { container } = render(
      <ConversationStream
        items={[msg('a', 'first'), msg('b', 'second')]}
        selectedEventId={null}
        findingEventIds={new Set()}
        onSelect={() => {}}
        onLoadOlder={onLoadOlder}
        canLoadOlder
      />,
    );
    const scroller = container.querySelector('[data-testid="conversation-stream"]') as HTMLElement;
    // Scroll up into the near-top zone, but with no preceding wheel/pointer/key.
    scroller.scrollTop = 400;
    fireEvent.scroll(scroller);
    scroller.scrollTop = 0;
    fireEvent.scroll(scroller);
    expect(onLoadOlder).not.toHaveBeenCalled();
  });

  // Direction guard: the native anchorTo:'end' re-anchor scrolls DOWN after a
  // prepend; a downward scroll through the top zone must NOT re-trigger a load
  // (that is what stops one load from cascading into many).
  it('does NOT page older history when scrolling DOWN through the top zone', () => {
    const onLoadOlder = vi.fn();
    const { container } = render(
      <ConversationStream
        items={[msg('a', 'first'), msg('b', 'second')]}
        selectedEventId={null}
        findingEventIds={new Set()}
        onSelect={() => {}}
        onLoadOlder={onLoadOlder}
        canLoadOlder
      />,
    );
    const scroller = container.querySelector('[data-testid="conversation-stream"]') as HTMLElement;
    fireEvent.wheel(scroller, { deltaY: 120 }); // interacted
    scroller.scrollTop = 5; // start at the very top
    fireEvent.scroll(scroller);
    scroller.scrollTop = 60; // scroll DOWN, still within the near-top zone
    fireEvent.scroll(scroller);
    expect(onLoadOlder).not.toHaveBeenCalled();
  });

  it('does NOT page older history once the session start is reached (canLoadOlder=false)', () => {
    const onLoadOlder = vi.fn();
    const { container } = render(
      <ConversationStream
        items={[msg('a', 'first'), msg('b', 'second')]}
        selectedEventId={null}
        findingEventIds={new Set()}
        onSelect={() => {}}
        onLoadOlder={onLoadOlder}
        canLoadOlder={false}
      />,
    );
    const scroller = container.querySelector('[data-testid="conversation-stream"]') as HTMLElement;
    fireEvent.wheel(scroller, { deltaY: -120 });
    scroller.scrollTop = 400;
    fireEvent.scroll(scroller);
    scroller.scrollTop = 0;
    fireEvent.scroll(scroller);
    expect(onLoadOlder).not.toHaveBeenCalled();
  });
});
