import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi } from 'vitest';
import { WorkflowEndCard } from '../WorkflowEndCard';
import type { WorkflowEndCard as Card } from '../streamModel';

const card = (over: Partial<Card> = {}): Card => ({
  type: 'workflow-end', id: 'wfend-1', workflowId: 'wf1', name: 'facts', color: '#ff8a4c',
  status: 'completed', summary: 'done well', endTimestamp: '2026-06-14T02:00:00Z',
  agentCount: 5, notificationEventId: 'noti-1', ...over,
});

describe('WorkflowEndCard', () => {
  it('shows name, status, summary, agent count (deterministic completion)', () => {
    render(<WorkflowEndCard card={card()} />);
    const el = screen.getByTestId('workflow-end-card');
    expect(el).toHaveTextContent('워크플로우 종료');
    expect(el).toHaveTextContent('facts');
    expect(screen.getByTestId('workflow-end-status')).toHaveTextContent('completed');
    expect(el).toHaveTextContent('done well');
    expect(el).toHaveTextContent('5 agents');
    expect(el).toHaveAttribute('data-status', 'done');
  });

  it('failed → data-status=fail', () => {
    render(<WorkflowEndCard card={card({ status: 'failed', summary: 'boom' })} />);
    expect(screen.getByTestId('workflow-end-card')).toHaveAttribute('data-status', 'fail');
    expect(screen.getByTestId('workflow-end-status')).toHaveTextContent('failed');
  });

  it('jump fires onSelect with the notification eventId', async () => {
    const u = userEvent.setup();
    const onSelect = vi.fn();
    render(<WorkflowEndCard card={card()} onSelect={onSelect} />);
    await u.click(screen.getByTestId('workflow-end-jump'));
    expect(onSelect).toHaveBeenCalledWith('noti-1');
  });

  it('the whole card is selectable (clicking it selects the notification)', async () => {
    const u = userEvent.setup();
    const onSelect = vi.fn();
    render(<WorkflowEndCard card={card()} onSelect={onSelect} />);
    await u.click(screen.getByTestId('workflow-end-card'));
    expect(onSelect).toHaveBeenCalledWith('noti-1');
  });
});
