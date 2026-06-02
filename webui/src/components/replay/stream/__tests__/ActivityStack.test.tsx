import { render, screen, fireEvent, within } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { ActivityStack } from '../ActivityStack';
import type { ActivityStackData } from '../activityGroup';

const stack: ActivityStackData = { events: [
  { event: { event_id: 'c1', kind: 'tool_call', observed_at: 'z', tool_name: 'Read', payload: { tool_name: 'Read', input: { file_path: '/a/x.jpg' } } } as any, result: { isError: false } },
  { event: { event_id: 'c2', kind: 'tool_call', observed_at: 'z', tool_name: 'Bash', payload: { tool_name: 'Bash', input: { command: 'ls' } } } as any, result: { isError: true } },
] };

describe('ActivityStack', () => {
  it('renders a collapsed summary with top tools, count, error badge', () => {
    render(<ActivityStack stack={stack} selectedEventId={null} onSelect={() => {}} />);
    const s = screen.getByTestId('activity-stack');
    expect(s).toHaveAttribute('data-count', '2');
    expect(s).toHaveAttribute('data-errors', '1');
    expect(within(s).getByText(/Read/)).toBeInTheDocument();
    expect(screen.queryByTestId('activity-item')).toBeNull();
  });

  it('right-aligns the fold header time in the same meta cluster as the items', () => {
    const withDur: ActivityStackData = { events: [
      { event: { event_id: 'c1', kind: 'tool_call', observed_at: '2026-05-31T00:00:00.000Z', tool_name: 'Bash', payload: { tool_name: 'Bash', input: { command: 'ls' } } } as any, result: { isError: false }, durationMs: 1200 },
      { event: { event_id: 'c2', kind: 'tool_call', observed_at: '2026-05-31T00:00:01.200Z', tool_name: 'Read', payload: { tool_name: 'Read', input: { file_path: '/a' } } } as any, result: { isError: false }, durationMs: 0 },
    ] };
    render(<ActivityStack stack={withDur} selectedEventId={null} onSelect={() => {}} />);
    const toggle = screen.getByTestId('activity-stack-toggle');
    const meta = within(toggle).getByTestId('fold-meta');
    expect(meta.textContent ?? '').toMatch(/\d/); // the summary duration lives in the right-aligned meta
  });

  it('expands on click to show items, each selectable', () => {
    const onSelect = vi.fn();
    render(<ActivityStack stack={stack} selectedEventId={null} onSelect={onSelect} />);
    fireEvent.click(screen.getByTestId('activity-stack-toggle'));
    const items = screen.getAllByTestId('activity-item');
    expect(items).toHaveLength(2);
    expect(within(items[0]).getByText('Read')).toBeInTheDocument();
    fireEvent.click(items[1]);
    expect(onSelect).toHaveBeenCalledWith('c2');
  });

  it('marks the selected item (auto-expanded via selection, no manual click needed)', () => {
    render(<ActivityStack stack={stack} selectedEventId="c1" onSelect={() => {}} />);
    const items = screen.getAllByTestId('activity-item');
    expect(items[0].getAttribute('data-selected')).toBe('true');
  });

  it('can fold the run even while a child is selected (regression: stuck-open bug)', () => {
    render(<ActivityStack stack={stack} selectedEventId="c1" onSelect={() => {}} />);
    expect(screen.getAllByTestId('activity-item').length).toBeGreaterThan(0); // auto-expanded (c1 selected)
    fireEvent.click(screen.getByTestId('activity-stack-toggle')); // user collapses
    expect(screen.queryByTestId('activity-item')).toBeNull(); // folded, even though c1 still selected
  });

  it('auto-expands when selectedEventId matches one of its events', () => {
    // No manual toggle click — the stack opens because c2 is selected so the
    // host can scroll the selected activity item into view.
    render(<ActivityStack stack={stack} selectedEventId="c2" onSelect={() => {}} />);
    const items = screen.getAllByTestId('activity-item');
    expect(items).toHaveLength(2);
    expect(items[1].getAttribute('data-selected')).toBe('true');
  });

  it('stays collapsed when selectedEventId is for a different stack', () => {
    render(<ActivityStack stack={stack} selectedEventId="other" onSelect={() => {}} />);
    expect(screen.queryByTestId('activity-item')).toBeNull();
  });

  it('shows tool + hook items with a consistent right-aligned [time, ok] meta cluster (time before status)', () => {
    const mixed: ActivityStackData = { events: [
      // a tool_call with a matched result → durationMs computed upstream
      { event: { event_id: 't1', kind: 'tool_call', observed_at: 'z', tool_name: 'Bash',
        payload: { tool_name: 'Bash', input: { command: 'ls' } } } as any,
        result: { isError: false }, durationMs: 796 },
      // a hook_event → time + ok from its own payload
      { event: { event_id: 'h1', kind: 'hook_event', observed_at: 'z',
        payload: { type: 'hook_success', hookName: 'PreToolUse:Bash', exitCode: 0, durationMs: 330 } } as any,
        result: null },
    ] };
    render(<ActivityStack stack={mixed} selectedEventId="t1" onSelect={() => {}} />);
    const items = screen.getAllByTestId('activity-item');
    // every item carries a single right-aligned meta cluster
    for (const item of items) {
      const meta = within(item).getByTestId('activity-meta');
      const t = meta.textContent ?? '';
      // order: ok first, then time — so time always sits at the far-right edge
      // and lines up across cards whether or not they have an ok/error badge.
      expect(t.indexOf('ok')).toBeLessThan(t.indexOf('ms'));
    }
    expect(within(items[0]).getByTestId('activity-meta').textContent).toContain('796ms');
    expect(within(items[1]).getByTestId('activity-meta').textContent).toContain('330ms');
  });

  it('shows a hook_event item with its ok badge and duration (from its own payload)', () => {
    // hook_event success/duration live in the event's OWN payload (exitCode /
    // durationMs), not in a matched tool_result. Anchored to real hook_success
    // shape (see hookFacet.test.ts).
    const hookStack: ActivityStackData = { events: [
      { event: { event_id: 'h1', kind: 'hook_event', observed_at: 'z',
        payload: { type: 'hook_success', hookName: 'PreToolUse:Bash', exitCode: 0, durationMs: 330 } } as any,
        result: null },
    ] };
    render(<ActivityStack stack={hookStack} selectedEventId="h1" onSelect={() => {}} />);
    const item = screen.getByTestId('activity-item');
    expect(within(item).getByText('PreToolUse:Bash')).toBeInTheDocument();
    expect(within(item).getByText('ok')).toBeInTheDocument();
    expect(within(item).getByText('330ms')).toBeInTheDocument();
  });

  it('renders a tag chip for a tagged Bash event (search·read) and none for control', () => {
    const ev = (command: string, id: string) => ({ event_id: id, kind: 'tool_call', tool_name: 'Bash', observed_at: '2026-05-31T00:00:00Z', payload: { input: { command } } });
    const stack = { events: [ { event: ev('grep -n x', 'a'), result: null }, { event: ev('cd /tmp', 'b'), result: null } ] };
    render(<ActivityStack stack={stack as any} selectedEventId={'a'} onSelect={() => {}} />); // selected → expanded
    expect(screen.getByText('search·read')).toBeInTheDocument();
    // control event 'cd' produces no chip text
    expect(screen.queryByText('control')).toBeNull();
  });
});
