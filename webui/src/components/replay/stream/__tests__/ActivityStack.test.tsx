import { render, screen, fireEvent, within } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { ActivityStack } from '../ActivityStack';
import type { ActivityStackData } from '../activityGroup';

const stack: ActivityStackData = { phase: 'exploration', events: [
  { event: { event_id: 'c1', kind: 'tool_call', observed_at: 'z', tool_name: 'Read', payload: { tool_name: 'Read', input: { file_path: '/a/x.jpg' } } } as any, result: { isError: false } },
  { event: { event_id: 'c2', kind: 'tool_call', observed_at: 'z', tool_name: 'Bash', payload: { tool_name: 'Bash', input: { command: 'ls' } } } as any, result: { isError: true } },
] };

describe('ActivityStack', () => {
  it('renders a collapsed summary with phase, top tools, count, error badge', () => {
    render(<ActivityStack stack={stack} selectedEventId={null} onSelect={() => {}} />);
    const s = screen.getByTestId('activity-stack');
    expect(s).toHaveAttribute('data-phase', 'exploration');
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
});
