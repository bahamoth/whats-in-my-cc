import { render, screen } from '@testing-library/react';
import { describe, it, expect } from 'vitest';
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
});
