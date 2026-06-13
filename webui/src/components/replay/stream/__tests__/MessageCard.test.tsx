import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
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
  it('shows a source badge revealing origin (external / subagent / agent)', () => {
    const { rerender } = render(<MessageCard item={m({ role: 'user' })} selected={false} onSelect={() => {}} />);
    expect(screen.getByTestId('source-badge')).toHaveTextContent('external');
    rerender(<MessageCard item={m({ role: 'user', sidechain: true })} selected={false} onSelect={() => {}} />);
    expect(screen.getByTestId('source-badge')).toHaveTextContent('subagent');
    rerender(<MessageCard item={m({ role: 'assistant', model: 'claude-opus-4-8' })} selected={false} onSelect={() => {}} />);
    expect(screen.getByTestId('source-badge')).toHaveTextContent('agent');
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

describe('MessageCard — markdown raw/styled view', () => {
  it('assistant messages render markdown by default (styled mode)', () => {
    const { container } = render(
      <MessageCard item={m({ role: 'assistant', model: 'claude-opus-4-8', text: '결론은 **중요**하다' })} selected={false} onSelect={() => {}} />,
    );
    expect(container.querySelector('strong')).toHaveTextContent('중요');
    expect(screen.getByTestId('message-bubble')).toHaveAttribute('data-mode', 'styled');
  });

  it('user messages default to raw (prompts are literal text)', () => {
    render(<MessageCard item={m({ role: 'user', text: '경로는 a_b_c와 **그대로**' })} selected={false} onSelect={() => {}} />);
    expect(screen.getByText('경로는 a_b_c와 **그대로**')).toBeInTheDocument();
    expect(screen.getByTestId('message-bubble')).toHaveAttribute('data-mode', 'raw');
  });

  it('the toggle flips styled ↔ raw per card', async () => {
    const user = userEvent.setup();
    const { container } = render(
      <MessageCard item={m({ role: 'assistant', model: 'claude-opus-4-8', text: '`code` 조각' })} selected={false} onSelect={() => {}} />,
    );
    expect(container.querySelector('code')).toHaveTextContent('code');
    await user.click(screen.getByTestId('md-toggle'));
    expect(container.querySelector('code')).toBeNull();
    expect(screen.getByText('`code` 조각')).toBeInTheDocument();
    await user.click(screen.getByTestId('md-toggle'));
    expect(container.querySelector('code')).toHaveTextContent('code');
  });

  it('clicking the toggle does NOT select the card', async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    render(
      <MessageCard item={m({ role: 'assistant', model: 'claude-opus-4-8', text: 'x' })} selected={false} onSelect={onSelect} />,
    );
    await user.click(screen.getByTestId('md-toggle'));
    expect(onSelect).not.toHaveBeenCalled();
  });

  it('styled mode renders GFM tables', () => {
    const { container } = render(
      <MessageCard
        item={m({ role: 'assistant', model: 'claude-opus-4-8', text: '| a | b |\n| - | - |\n| 1 | 2 |' })}
        selected={false}
        onSelect={() => {}}
      />,
    );
    expect(container.querySelector('table')).not.toBeNull();
  });
});
