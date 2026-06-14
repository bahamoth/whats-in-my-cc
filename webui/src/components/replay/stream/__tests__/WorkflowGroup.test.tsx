import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { WorkflowGroup } from '../WorkflowGroup';
import type { WorkflowGroup as WG, SidechainGroup } from '../streamModel';

const child = (id: string, end: string): SidechainGroup => ({
  type: 'sidechain-group', id, agentId: id, agentType: 'Explore', description: id + ' 작업', taskEventId: null, conclusion: id + ' 결론',
  items: [
    { type: 'message', id: id + 'u', eventId: id + 'u', role: 'user', model: null, text: 'prompt', timestamp: '2026-06-14T00:00:00Z', sidechain: true },
    { type: 'message', id: id + 'a', eventId: id + 'a', role: 'assistant', model: null, text: id + ' 결론', timestamp: end, sidechain: true },
  ],
});
const wg: WG = { type: 'workflow-group', id: 'wf1', name: 'review-changes', description: null, taskEventId: 'wfc',
  agentGroups: [child('A', '2026-06-14T00:38:00Z'), child('B', '2026-06-14T00:02:00Z')], synthesis: '종합 결과 X', settled: true };

const noop = () => {};
describe('WorkflowGroup', () => {
  it('접힘 기본: 워크플로우명·종합·통계칩·미니간트 모두 노출, 자식 행 숨김', () => {
    render(<WorkflowGroup group={wg} selectedEventId={null} onSelect={noop} findingEventIds={new Set()} />);
    expect(screen.getByTestId('workflow-group')).toHaveAttribute('data-expanded', 'false');
    expect(screen.getByText('review-changes')).toBeInTheDocument();
    expect(screen.getByTestId('wf-synthesis')).toHaveTextContent('종합');
    expect(screen.getByTestId('wf-stats')).toHaveTextContent('최대 병렬');
    expect(screen.getAllByTestId('wf-lane').length).toBe(2);   // 미니간트 레인 = 에이전트 수
    expect(screen.queryByTestId('subagent-group')).toBeNull(); // 자식은 펼쳐야
  });
  it('펼치면 자식 SubagentGroup 노출', () => {
    render(<WorkflowGroup group={wg} selectedEventId={null} onSelect={noop} findingEventIds={new Set()} />);
    fireEvent.click(screen.getByTestId('wf-toggle'));
    expect(screen.getAllByTestId('subagent-group').length).toBe(2);
  });
  it('concurrentMainCount>0이면 "main N건 동시" 배지', () => {
    render(
      <WorkflowGroup group={{ ...wg, concurrentMainCount: 3 }} selectedEventId={null} onSelect={noop} findingEventIds={new Set()} />,
    );
    expect(screen.getByTestId('wf-concurrent')).toHaveTextContent('main 3건 동시');
  });
  it('호출 버튼이 Workflow tool_call을 선택 — 시작 카드 선택 가능', () => {
    const sel = vi.fn();
    render(<WorkflowGroup group={wg} selectedEventId={null} onSelect={sel} findingEventIds={new Set()} />);
    fireEvent.click(screen.getByTestId('wf-jump'));
    expect(sel).toHaveBeenCalledWith('wfc');
  });
});
