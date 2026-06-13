import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect } from 'vitest';
import { SubagentGroup } from '../SubagentGroup';
import type { SidechainGroup } from '../streamModel';

const group: SidechainGroup = {
  type: 'sidechain-group',
  id: 'sc-1',
  agentId: 'a3f9c2d41b',
  items: [
    { type: 'message', id: 'p', eventId: 'p', role: 'user', model: null, text: '서브 프롬프트 첫 줄\n둘째 줄', timestamp: '2026-05-28T00:00:00Z', sidechain: true },
    {
      type: 'activity-run',
      id: 'run-c1',
      events: [
        {
          event: { event_id: 'c1', raw_event_id: '', session_id: 's', event_uuid: null, parent_uuid: null, observed_at: '2026-05-28T00:00:05Z', actor: 'assistant', kind: 'tool_call', subkind: null, tool_use_id: 'u1', tool_name: 'Read', turn_id: null, is_sidechain: true, is_meta: false, payload: { tool_name: 'Read', input: { file_path: '/a' } } },
          result: { isError: false },
          durationMs: 200,
        },
      ],
    },
    { type: 'message', id: 'r', eventId: 'r', role: 'assistant', model: 'claude-opus-4-8', text: '서브 응답', timestamp: '2026-05-28T00:00:42Z', sidechain: true },
  ],
};

describe('SubagentGroup', () => {
  it('is collapsed by default: summary header visible, inner exchange hidden', () => {
    render(
      <SubagentGroup group={group} selectedEventId={null} onSelect={() => {}} findingEventIds={new Set()} />,
    );
    expect(screen.getByText('Subagent')).toBeInTheDocument();
    // identity + overview live on the header
    expect(screen.getByTestId('subagent-agent-chip')).toHaveTextContent('a3f9c2');
    expect(screen.getByTestId('subagent-preview')).toHaveTextContent('서브 프롬프트 첫 줄');
    const meta = screen.getByTestId('subagent-meta');
    expect(meta).toHaveTextContent('메시지 2');
    expect(meta).toHaveTextContent('도구 1');
    expect(meta).toHaveTextContent('42.0s'); // 00:00:00 → 00:00:42
    // children stay unmounted while collapsed
    expect(screen.queryByText('서브 응답')).toBeNull();
  });

  it('expands on toggle, showing the inner exchange, and collapses back', async () => {
    const user = userEvent.setup();
    render(
      <SubagentGroup group={group} selectedEventId={null} onSelect={() => {}} findingEventIds={new Set()} />,
    );
    const toggle = screen.getByTestId('subagent-toggle');
    expect(toggle).toHaveAttribute('aria-expanded', 'false');
    await user.click(toggle);
    expect(toggle).toHaveAttribute('aria-expanded', 'true');
    // the dispatched prompt (full text — not just the header preview) and the
    // subagent's reply both render inside
    expect(screen.getByText(/둘째 줄/)).toBeInTheDocument();
    expect(screen.getByText('서브 응답')).toBeInTheDocument();
    // the prompt is labelled "Prompt" (not "You"), the reply keeps its model name
    expect(screen.getByText('Prompt')).toBeInTheDocument();
    expect(screen.queryByText('You')).toBeNull();
    await user.click(toggle);
    expect(screen.queryByText('서브 응답')).toBeNull();
  });

  it('auto-expands when the selected event lives inside, and can still be collapsed', async () => {
    const user = userEvent.setup();
    render(
      <SubagentGroup group={group} selectedEventId="r" onSelect={() => {}} findingEventIds={new Set()} />,
    );
    expect(screen.getByText('서브 응답')).toBeInTheDocument();
    // explicit user collapse wins even while a child is selected
    await user.click(screen.getByTestId('subagent-toggle'));
    expect(screen.queryByText('서브 응답')).toBeNull();
  });

  it('omits the agent chip when the group has no agent attribution', () => {
    render(
      <SubagentGroup
        group={{ ...group, agentId: null }}
        selectedEventId={null}
        onSelect={() => {}}
        findingEventIds={new Set()}
      />,
    );
    expect(screen.queryByTestId('subagent-agent-chip')).toBeNull();
  });
});
