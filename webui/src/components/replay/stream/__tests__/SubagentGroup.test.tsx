import { screen } from '@testing-library/react';
import { renderWithI18n as render } from '../../../../test/i18nRender';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi } from 'vitest';
import { SubagentGroup } from '../SubagentGroup';
import type { SidechainGroup } from '../streamModel';
import { agentColor } from '../../../../lib/colorHash';

const group: SidechainGroup = {
  type: 'sidechain-group',
  id: 'sc-1',
  agentId: 'a3f9c2d41b',
  agentType: null,
  description: null,
  taskEventId: null,
  conclusion: null,
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

  it('shows the agent TYPE when known, and prefers the Task description as preview', () => {
    render(
      <SubagentGroup
        group={{ ...group, agentType: 'Explore', description: '간단 조사' }}
        selectedEventId={null}
        onSelect={() => {}}
        findingEventIds={new Set()}
      />,
    );
    expect(screen.getByTestId('subagent-type')).toHaveTextContent('Explore');
    // 사이드카 description이 프롬프트 첫 줄보다 우선한다
    expect(screen.getByTestId('subagent-preview')).toHaveTextContent('간단 조사');
  });

  it('jump-to-Task button selects the dispatching Task event without toggling the fold', async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    render(
      <SubagentGroup
        group={{ ...group, taskEventId: 'task-ev-1' }}
        selectedEventId={null}
        onSelect={onSelect}
        findingEventIds={new Set()}
      />,
    );
    const jump = screen.getByTestId('subagent-jump');
    await user.click(jump);
    expect(onSelect).toHaveBeenCalledWith('task-ev-1');
    // 점프가 fold 상태를 건드리지 않는다 — 여전히 접힘
    expect(screen.getByTestId('subagent-toggle')).toHaveAttribute('aria-expanded', 'false');
  });

  it('omits the jump button when the Task event is not in the loaded window', () => {
    render(
      <SubagentGroup group={group} selectedEventId={null} onSelect={() => {}} findingEventIds={new Set()} />,
    );
    expect(screen.queryByTestId('subagent-jump')).toBeNull();
  });

  it('shows the conclusion line in the collapsed summary when present', () => {
    render(
      <SubagentGroup
        group={{ ...group, conclusion: '핵심 결론' }}
        selectedEventId={null}
        onSelect={() => {}}
        findingEventIds={new Set()}
      />,
    );
    // collapsed by default, yet the conclusion is visible (sits below the header)
    expect(screen.getByTestId('subagent-toggle')).toHaveAttribute('aria-expanded', 'false');
    expect(screen.getByTestId('subagent-conclusion')).toHaveTextContent('핵심 결론');
  });

  it('omits the conclusion line when the group has no conclusion', () => {
    render(
      <SubagentGroup
        group={{ ...group, conclusion: null }}
        selectedEventId={null}
        onSelect={() => {}}
        findingEventIds={new Set()}
      />,
    );
    expect(screen.queryByTestId('subagent-conclusion')).toBeNull();
  });
});

describe('SubagentGroup color identity (hairline gutter)', () => {
  it('sets --agentColor from hash(agentId) and shows a color swatch', () => {
    render(
      <SubagentGroup group={group} selectedEventId={null} onSelect={() => {}} findingEventIds={new Set()} />,
    );
    const section = screen.getByTestId('subagent-group');
    expect(section.style.getPropertyValue('--agentColor')).toBe(agentColor('a3f9c2d41b'));
    expect(screen.getByTestId('subagent-swatch')).toBeInTheDocument();
  });

  it('agentId null → neutral var, swatch still present (graceful)', () => {
    render(
      <SubagentGroup
        group={{ ...group, agentId: null }}
        selectedEventId={null}
        onSelect={() => {}}
        findingEventIds={new Set()}
      />,
    );
    expect(screen.getByTestId('subagent-group').style.getPropertyValue('--agentColor')).toBe(
      agentColor(null),
    );
    expect(screen.getByTestId('subagent-swatch')).toBeInTheDocument();
  });
});

describe('SubagentGroup — status pill + end-card-aware conclusion', () => {
  it('shows "실행 중" when no conclusion, "완료" when concluded', () => {
    const { rerender } = render(
      <SubagentGroup group={{ ...group, conclusion: null }} selectedEventId={null} onSelect={() => {}} findingEventIds={new Set()} />,
    );
    expect(screen.getByTestId('subagent-status')).toHaveTextContent('실행 중');
    rerender(
      <SubagentGroup group={{ ...group, conclusion: '결론' }} selectedEventId={null} onSelect={() => {}} findingEventIds={new Set()} />,
    );
    expect(screen.getByTestId('subagent-status')).toHaveTextContent('완료');
  });

  it('hides the inline conclusion when an end card carries it (hasEndCard)', () => {
    const { rerender } = render(
      <SubagentGroup group={{ ...group, conclusion: '핵심 결론' }} selectedEventId={null} onSelect={() => {}} findingEventIds={new Set()} />,
    );
    // no end card → conclusion stays on the start card
    expect(screen.getByTestId('subagent-conclusion')).toHaveTextContent('핵심 결론');
    rerender(
      <SubagentGroup group={{ ...group, conclusion: '핵심 결론', hasEndCard: true }} selectedEventId={null} onSelect={() => {}} findingEventIds={new Set()} />,
    );
    // end card present → start card drops the (duplicated) conclusion line
    expect(screen.queryByTestId('subagent-conclusion')).toBeNull();
  });
});
