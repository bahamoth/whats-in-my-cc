// webui/src/components/replay/stream/__tests__/BatchGroup.test.tsx
// BatchGroup renders one parallel-dispatch batch as a 2-level collapsible block:
//  - L0 (collapsed, default): batch identity (chip, N agents, status, total
//    span) + the synthesis line, children hidden.
//  - L1 (expanded): each child SubagentGroup + a bottom outcome (synthesis) line.
// Same prop signature as SubagentGroup; uses the de-interleaved agentGroups.
import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { BatchGroup } from '../BatchGroup';
import type { BatchGroup as BatchGroupModel, SidechainGroup } from '../streamModel';

function childGroup(over: Partial<SidechainGroup> = {}): SidechainGroup {
  return {
    type: 'sidechain-group',
    id: 'sc-A',
    agentId: 'A',
    agentType: 'Explore',
    description: '조사 A',
    taskEventId: null,
    conclusion: 'A 결론',
    items: [
      {
        type: 'message',
        id: 'aMsg',
        eventId: 'aMsg',
        role: 'assistant',
        model: null,
        text: 'A 결론',
        timestamp: '2026-06-13T00:00:30Z',
        sidechain: true,
      },
    ],
    ...over,
  };
}

const fixtureBatch: BatchGroupModel = {
  type: 'batch-group',
  id: 'batch-sc-A',
  agentGroups: [
    childGroup({ id: 'sc-A', agentId: 'A', description: '조사 A', conclusion: 'A 결론' }),
    childGroup({ id: 'sc-B', agentId: 'B', description: '조사 B', conclusion: 'B 결론' }),
  ],
  dispatchMessageId: 'm1',
  synthesis: '두 결과를 종합하면 X',
  settled: true,
};

describe('BatchGroup', () => {
  it('L0 접힘: 배치 식별 + 종합 결과 보임, 자식 숨김', () => {
    render(
      <BatchGroup
        group={fixtureBatch}
        selectedEventId={null}
        onSelect={() => {}}
        findingEventIds={new Set()}
      />,
    );
    const root = screen.getByTestId('batch-group');
    expect(root).toHaveAttribute('data-expanded', 'false');
    // batch identity chip
    expect(screen.getByText('병렬 배치')).toBeInTheDocument();
    // N agents
    expect(screen.getByTestId('batch-meta')).toHaveTextContent('2');
    // synthesis line visible at L0
    expect(screen.getByTestId('batch-synthesis')).toHaveTextContent('두 결과를 종합하면 X');
    // children stay unmounted while collapsed
    expect(screen.queryByTestId('subagent-group')).toBeNull();
  });

  it('settled면 ✓ N/N, 진행 중이면 ⏳ + 종합 자리 "진행 중"', () => {
    const running: BatchGroupModel = {
      ...fixtureBatch,
      synthesis: null,
      settled: false,
      agentGroups: [
        childGroup({ id: 'sc-A', agentId: 'A', conclusion: 'A 결론' }),
        childGroup({ id: 'sc-B', agentId: 'B', conclusion: null }),
      ],
    };
    render(
      <BatchGroup
        group={running}
        selectedEventId={null}
        onSelect={() => {}}
        findingEventIds={new Set()}
      />,
    );
    // not-settled defaults to expanded (progress aid)
    expect(screen.getByTestId('batch-group')).toHaveAttribute('data-expanded', 'true');
    expect(screen.getByTestId('batch-status')).toHaveTextContent('⏳');
    // synthesis missing → "진행 중"
    expect(screen.getByTestId('batch-synthesis')).toHaveTextContent('진행 중');
  });

  it('settled면 상태가 ✓ N/N', () => {
    render(
      <BatchGroup
        group={fixtureBatch}
        selectedEventId={null}
        onSelect={() => {}}
        findingEventIds={new Set()}
      />,
    );
    expect(screen.getByTestId('batch-status')).toHaveTextContent('✓ 2/2');
  });

  it('펼치면 자식 SubagentGroup 들 보임', () => {
    render(
      <BatchGroup
        group={fixtureBatch}
        selectedEventId={null}
        onSelect={() => {}}
        findingEventIds={new Set()}
      />,
    );
    fireEvent.click(screen.getByTestId('batch-toggle'));
    expect(screen.getByTestId('batch-group')).toHaveAttribute('data-expanded', 'true');
    expect(screen.getAllByTestId('subagent-group')).toHaveLength(fixtureBatch.agentGroups.length);
  });

  it('auto-expands when the selected event lives inside a child agent', () => {
    render(
      <BatchGroup
        group={fixtureBatch}
        selectedEventId="aMsg"
        onSelect={() => {}}
        findingEventIds={new Set()}
      />,
    );
    expect(screen.getByTestId('batch-group')).toHaveAttribute('data-expanded', 'true');
  });

  it('forwards onSelect from a child to the host', () => {
    const onSelect = vi.fn();
    render(
      <BatchGroup
        group={fixtureBatch}
        selectedEventId="aMsg"
        onSelect={onSelect}
        findingEventIds={new Set()}
      />,
    );
    // expanded (selection inside) → the child message card is clickable
    screen.getAllByTestId('message-card')[0].click();
    expect(onSelect).toHaveBeenCalledWith('aMsg');
  });
});
