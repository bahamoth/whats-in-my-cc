import { screen } from '@testing-library/react';
import { renderWithI18n as render } from '../../../../test/i18nRender';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi } from 'vitest';
import { SubagentEndCard } from '../SubagentEndCard';
import type { SubagentEndCard as EndCard } from '../streamModel';

const card: EndCard = {
  type: 'subagent-end',
  id: 'end-aa1844',
  agentId: 'aa1844',
  color: '#7da7ff',
  conclusion: 'GREEN confirmed. 4 tests pass',
  durationMs: 79000,
  messageCount: 63,
  toolCount: 139,
  endTimestamp: '2026-06-14T01:42:24Z',
};

describe('SubagentEndCard', () => {
  it('shows 종료 + counts + conclusion (result without expanding)', () => {
    render(<SubagentEndCard card={card} />);
    const el = screen.getByTestId('subagent-end-card');
    expect(el).toHaveTextContent('종료');
    expect(el).toHaveTextContent('GREEN confirmed. 4 tests pass');
    expect(el).toHaveTextContent('63');
    expect(el).toHaveTextContent('139');
    expect(el).toHaveTextContent('결론');
  });

  it('with a matched task-notification: status pill + jump to 원문 (Agent run_in_background)', async () => {
    const u = userEvent.setup();
    const onSelect = vi.fn();
    render(<SubagentEndCard card={{ ...card, status: 'failed', notificationEventId: 'noti-a' }} onSelect={onSelect} />);
    expect(screen.getByTestId('subagent-end-card')).toHaveAttribute('data-status', 'fail');
    expect(screen.getByTestId('subagent-end-status')).toHaveTextContent('failed');
    await u.click(screen.getByTestId('subagent-end-jump'));
    expect(onSelect).toHaveBeenCalledWith('noti-a');
  });
});
