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

  it('marks the selected item', () => {
    render(<ActivityStack stack={stack} selectedEventId="c1" onSelect={() => {}} />);
    fireEvent.click(screen.getByTestId('activity-stack-toggle'));
    const items = screen.getAllByTestId('activity-item');
    expect(items[0].getAttribute('data-selected')).toBe('true');
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

  it('renders a tag chip for a tagged Bash event (search·read) and none for control', () => {
    const ev = (command: string, id: string) => ({ event_id: id, kind: 'tool_call', tool_name: 'Bash', observed_at: '2026-05-31T00:00:00Z', payload: { input: { command } } });
    const stack = { events: [ { event: ev('grep -n x', 'a'), result: null }, { event: ev('cd /tmp', 'b'), result: null } ] };
    render(<ActivityStack stack={stack as any} selectedEventId={'a'} onSelect={() => {}} />); // selected → expanded
    expect(screen.getByText('search·read')).toBeInTheDocument();
    // control event 'cd' produces no chip text
    expect(screen.queryByText('control')).toBeNull();
  });
});
