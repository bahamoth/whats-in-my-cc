import { render, screen } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { MessageCard } from '../MessageCard';
import type { MessageItem } from '../streamModel';

const m = (over: Partial<MessageItem>): MessageItem => ({ type: 'message', id: 'x', eventId: 'x', role: 'user', model: null, text: 'hi', timestamp: '2026-05-28T09:14:02Z', sidechain: false, ...over });

describe('MessageCard', () => {
  it('user message aligns right with You label', () => {
    render(<MessageCard item={m({ role: 'user', text: '질문' })} selected={false} onSelect={() => {}} />);
    const c = screen.getByTestId('message-card');
    expect(c).toHaveAttribute('data-role', 'user');
    expect(c).toHaveAttribute('data-align', 'right');
    expect(screen.getByText('You')).toBeInTheDocument();
    expect(screen.getByText('질문')).toBeInTheDocument();
  });
  it('assistant message aligns left with model name', () => {
    render(<MessageCard item={m({ role: 'assistant', model: 'claude-opus-4-8', text: '답변' })} selected={false} onSelect={() => {}} />);
    const c = screen.getByTestId('message-card');
    expect(c).toHaveAttribute('data-role', 'assistant');
    expect(c).toHaveAttribute('data-align', 'left');
    expect(screen.getByText('Opus 4.8')).toBeInTheDocument();
  });
  it('sidechain user message is the subagent prompt: "Prompt" label, left, not You', () => {
    render(<MessageCard item={m({ role: 'user', sidechain: true, text: '서브 프롬프트' })} selected={false} onSelect={() => {}} />);
    const c = screen.getByTestId('message-card');
    expect(c).toHaveAttribute('data-align', 'left');
    expect(screen.getByText('Prompt')).toBeInTheDocument();
    expect(screen.queryByText('You')).toBeNull();
  });
  it('thinking is left + distinct (data-role=thinking)', () => {
    render(<MessageCard item={m({ role: 'thinking', text: '추론중' })} selected={false} onSelect={() => {}} />);
    expect(screen.getByTestId('message-card')).toHaveAttribute('data-role', 'thinking');
  });
  it('forwards click with eventId', () => {
    const onSelect = vi.fn();
    render(<MessageCard item={m({ eventId: 'e1' })} selected={false} onSelect={onSelect} />);
    screen.getByTestId('message-card').click();
    expect(onSelect).toHaveBeenCalledWith('e1');
  });
  it('shows a finding marker only when hasFinding', () => {
    const { rerender } = render(<MessageCard item={m({})} selected={false} onSelect={() => {}} />);
    expect(screen.queryByLabelText(/finding/i)).toBeNull();
    rerender(<MessageCard item={m({})} selected={false} onSelect={() => {}} hasFinding />);
    expect(screen.getByLabelText(/finding/i)).toBeInTheDocument();
  });
});
