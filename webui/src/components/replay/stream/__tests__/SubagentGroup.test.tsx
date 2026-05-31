import { render, screen } from '@testing-library/react';
import { describe, it, expect } from 'vitest';
import { SubagentGroup } from '../SubagentGroup';
import type { SidechainGroup } from '../streamModel';

const group: SidechainGroup = {
  type: 'sidechain-group',
  id: 'sc-1',
  items: [
    { type: 'message', id: 'p', eventId: 'p', role: 'user', model: null, text: '서브 프롬프트', timestamp: '2026-05-28T00:00:00Z', sidechain: true },
    { type: 'message', id: 'r', eventId: 'r', role: 'assistant', model: 'claude-opus-4-8', text: '서브 응답', timestamp: '2026-05-28T00:00:01Z', sidechain: true },
  ],
};

describe('SubagentGroup', () => {
  it('renders a labelled Subagent container with the inner exchange', () => {
    render(
      <SubagentGroup
        group={group}
        selectedEventId={null}
        onSelect={() => {}}
        findingEventIds={new Set()}
      />,
    );
    expect(screen.getByTestId('subagent-group')).toBeInTheDocument();
    // group identity is shown once, on the container header — not on each child
    expect(screen.getByText('Subagent')).toBeInTheDocument();
    // the dispatched prompt and the subagent's reply both render inside
    expect(screen.getByText('서브 프롬프트')).toBeInTheDocument();
    expect(screen.getByText('서브 응답')).toBeInTheDocument();
    // the prompt is labelled "Prompt" (not "You"), the reply keeps its model name
    expect(screen.getByText('Prompt')).toBeInTheDocument();
    expect(screen.queryByText('You')).toBeNull();
  });
});
