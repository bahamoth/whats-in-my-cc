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
  it('source badge reflects deterministic origin (human/command/skill/subagent/agent), not the meaningless hardcoded "external"', () => {
    const { rerender } = render(<MessageCard item={m({ role: 'user', origin: 'human' })} selected={false} onSelect={() => {}} />);
    expect(screen.getByTestId('source-badge')).toHaveTextContent('human');
    expect(screen.queryByText('external')).toBeNull();
    rerender(<MessageCard item={m({ role: 'user', origin: 'command', commandName: '/model', text: '<command-name>/model</command-name>' })} selected={false} onSelect={() => {}} />);
    expect(screen.getByTestId('source-badge')).toHaveTextContent('command');
    rerender(<MessageCard item={m({ role: 'user', origin: 'skill', text: 'skill body' })} selected={false} onSelect={() => {}} />);
    expect(screen.getByTestId('source-badge')).toHaveTextContent('skill');
    rerender(<MessageCard item={m({ role: 'user', sidechain: true })} selected={false} onSelect={() => {}} />);
    expect(screen.getByTestId('source-badge')).toHaveTextContent('subagent');
    rerender(<MessageCard item={m({ role: 'assistant', model: 'claude-opus-4-8' })} selected={false} onSelect={() => {}} />);
    expect(screen.getByTestId('source-badge')).toHaveTextContent('agent');
  });

  it('a user-invoked command/skill stays on the USER side (right), not "You", labelled by origin', () => {
    const { rerender } = render(
      <MessageCard
        item={m({ role: 'user', origin: 'command', commandName: '/model', text: '<command-name>/model</command-name>\n<command-args>opus</command-args>' })}
        selected={false}
        onSelect={() => {}}
      />,
    );
    const card = screen.getByTestId('message-card');
    expect(card).toHaveAttribute('data-align', 'right'); // user-originated → user side
    expect(card).toHaveAttribute('data-origin', 'command');
    expect(screen.queryByText('You')).toBeNull();
    expect(screen.getByText('/model')).toBeInTheDocument();
    // scaffolding XML is cleaned to "/name args", not shown raw
    expect(screen.getByTestId('message-bubble')).toHaveTextContent('/model opus');
    expect(screen.queryByText(/command-name/)).toBeNull();

    rerender(<MessageCard item={m({ role: 'user', origin: 'skill', text: 'injected skill body' })} selected={false} onSelect={() => {}} />);
    expect(screen.getByTestId('message-card')).toHaveAttribute('data-align', 'right');
    expect(screen.getByText('Skill')).toBeInTheDocument();
  });

  it('injected skill body collapses by default (reference, not conversation), even when short', () => {
    render(<MessageCard item={m({ role: 'user', origin: 'skill', text: '한 줄짜리 짧은 스킬 본문' })} selected={false} onSelect={() => {}} />);
    expect(screen.getByTestId('message-bubble')).toHaveAttribute('data-clamped', 'true');
    expect(screen.getByTestId('clamp-toggle')).toHaveTextContent('더 보기');
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

describe('MessageCard — header time is the VIEWER-LOCAL clock', () => {
  it('renders HH:MM:SS in the local timezone, not UTC (bug: toISOString despite the locale comment)', () => {
    const prev = process.env.TZ;
    process.env.TZ = 'Asia/Seoul'; // Node 13+ re-reads TZ on assignment
    try {
      render(<MessageCard item={m({ timestamp: '2026-05-28T00:00:00Z' })} selected={false} onSelect={() => {}} />);
      expect(screen.getByText('09:00:00')).toBeInTheDocument(); // UTC+9
    } finally {
      process.env.TZ = prev;
    }
  });
});

describe('MessageCard — long message clamp (더 보기)', () => {
  const longText = Array.from({ length: 80 }, (_, i) => `줄 ${i}: 본문이 아주 길다`).join('\n');

  it('clamps a long message by default and expands via 더 보기', async () => {
    const user = userEvent.setup();
    render(<MessageCard item={m({ role: 'user', text: longText })} selected={false} onSelect={() => {}} />);
    expect(screen.getByTestId('message-bubble')).toHaveAttribute('data-clamped', 'true');
    const more = screen.getByTestId('clamp-toggle');
    expect(more).toHaveTextContent('더 보기');
    await user.click(more);
    expect(screen.getByTestId('message-bubble')).toHaveAttribute('data-clamped', 'false');
    expect(screen.getByTestId('clamp-toggle')).toHaveTextContent('접기');
  });

  it('short messages never clamp and show no toggle', () => {
    render(<MessageCard item={m({ role: 'user', text: '짧은 메시지' })} selected={false} onSelect={() => {}} />);
    expect(screen.getByTestId('message-bubble')).not.toHaveAttribute('data-clamped', 'true');
    expect(screen.queryByTestId('clamp-toggle')).toBeNull();
  });

  it('clicking 더 보기 does NOT select the card', async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    render(<MessageCard item={m({ role: 'user', text: longText })} selected={false} onSelect={onSelect} />);
    await user.click(screen.getByTestId('clamp-toggle'));
    expect(onSelect).not.toHaveBeenCalled();
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
