import { describe, it, expect, vi } from 'vitest';
import { screen, fireEvent } from '@testing-library/react';
import { renderWithI18n as render } from '../../../../test/i18nRender';
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
  it('S5: 레인 클릭 시 그 에이전트만 인라인 드릴 (전체 펼침 아님)', () => {
    render(<WorkflowGroup group={wg} selectedEventId={null} onSelect={noop} findingEventIds={new Set()} />);
    fireEvent.click(screen.getAllByTestId('wf-lane')[0]);
    expect(screen.getAllByTestId('subagent-group').length).toBe(1);
    expect(screen.getByTestId('workflow-group')).toHaveAttribute('data-expanded', 'false');
    expect(screen.getByText('A 결론')).toBeInTheDocument();
    expect(screen.queryByText('B 결론')).toBeNull();
  });
  it('S5: 드릴된 레인 재클릭하면 접힘', () => {
    render(<WorkflowGroup group={wg} selectedEventId={null} onSelect={noop} findingEventIds={new Set()} />);
    const lane0 = () => screen.getAllByTestId('wf-lane')[0];
    fireEvent.click(lane0());
    expect(screen.getAllByTestId('subagent-group').length).toBe(1);
    fireEvent.click(lane0());
    expect(screen.queryByTestId('subagent-group')).toBeNull();
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

  it('단일 에이전트 워크플로우는 평탄화: 래퍼 토글·간트·통계 없이 자식을 직접 노출', () => {
    // N=1이면 1-바 간트·"최대 병렬 1"·이중 chevron이 무의미하다. 워크플로우 정체성
    // (이름·종합·호출 점프)은 한 줄로 남기고 자식 SubagentGroup을 바로 노출한다.
    const single: WG = { ...wg, agentGroups: [child('A', '2026-06-14T00:38:00Z')] };
    render(<WorkflowGroup group={single} selectedEventId={null} onSelect={noop} findingEventIds={new Set()} />);
    expect(screen.queryByTestId('wf-toggle')).toBeNull(); // 래퍼 chevron 없음
    expect(screen.queryByTestId('wf-lane')).toBeNull(); // 1-바 간트 없음
    expect(screen.queryByTestId('wf-stats')).toBeNull(); // 통계 없음
    expect(screen.getByTestId('subagent-group')).toBeInTheDocument(); // 자식 직접 노출
    expect(screen.getByText('review-changes')).toBeInTheDocument(); // 워크플로우명 유지
  });

  it('단일 에이전트 평탄화에서도 호출 점프는 유지', () => {
    const sel = vi.fn();
    const single: WG = { ...wg, agentGroups: [child('A', '2026-06-14T00:38:00Z')] };
    render(<WorkflowGroup group={single} selectedEventId={null} onSelect={sel} findingEventIds={new Set()} />);
    fireEvent.click(screen.getByTestId('wf-jump'));
    expect(sel).toHaveBeenCalledWith('wfc');
  });
});
