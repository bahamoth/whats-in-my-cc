// FilterBar — Task 10 (스펙 §1.4). Controlled component: 축별 칩 드롭다운 +
// 텍스트 검색(디바운스) + 활성 조건 제거형 칩 + "N건 매칭" + 점프-해제 알림.
import { useState } from 'react';
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { act, fireEvent, screen } from '@testing-library/react';
import { renderWithI18n as render } from '../../../../test/i18nRender';
import '@testing-library/jest-dom/vitest';
import { FilterBar } from '../FilterBar';
import { EMPTY_FILTER, type FilterState } from '../filterState';

// 실제 부모처럼 onChange를 state로 반영하는 controlled 하네스 — stale-closure
// race는 mid-debounce onChange가 새 filter로 재렌더될 때만 재현되므로, spy
// onChange(무재렌더)로는 잡히지 않는다. 최신 filter를 `seen`으로 노출한다.
function StatefulFilterBar({ initial, seen }: { initial: FilterState; seen: (f: FilterState) => void }) {
  const [filter, setFilter] = useState(initial);
  seen(filter);
  return <FilterBar filter={filter} onChange={setFilter} matchedCount={null} notice={null} />;
}

describe('FilterBar', () => {
  beforeEach(() => {
    // FilterBar's own cleanup effect(s) don't depend on timers, but tests below
    // toggle fake timers — keep each test isolated.
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('renders active chips, matched count, and emits onChange on chip removal', () => {
    const onChange = vi.fn();
    render(
      <FilterBar
        filter={{ ...EMPTY_FILTER, tools: ['Bash'], error: true, q: 'panic' }}
        onChange={onChange}
        matchedCount={7}
        notice={null}
      />,
    );
    expect(screen.getByText(/7건 매칭|7 matched/)).toBeInTheDocument();
    // Bash 칩 제거 → tools 빠진 상태로 onChange
    fireEvent.click(screen.getByRole('button', { name: /Bash/ }));
    expect(onChange).toHaveBeenCalledWith(expect.objectContaining({ tools: [], error: true }));
  });

  it('debounces text input by 300ms', async () => {
    vi.useFakeTimers();
    const onChange = vi.fn();
    render(<FilterBar filter={EMPTY_FILTER} onChange={onChange} matchedCount={null} notice={null} />);
    fireEvent.change(screen.getByPlaceholderText(/텍스트 검색|search text/), { target: { value: 'oom' } });
    expect(onChange).not.toHaveBeenCalled();
    await act(() => vi.advanceTimersByTime(320));
    expect(onChange).toHaveBeenCalledWith(expect.objectContaining({ q: 'oom' }));
    vi.useRealTimers();
  });

  it('preserves a mid-debounce filter change (stale-closure race regression)', async () => {
    // 사용자가 검색창에 타이핑(300ms 타이머 예약) → 만료 전 다른 축을 토글하면
    // (Bash 칩 제거) → 만료 시 debounce 콜백이 stale한 pre-click filter로
    // onChange를 발화해 그 토글을 조용히 되돌리던 데이터 손실(coordinator
    // Important). 콜백이 ref로 최신 filter를 읽으면 토글이 보존된다.
    vi.useFakeTimers();
    let current: FilterState = EMPTY_FILTER;
    render(
      <StatefulFilterBar
        initial={{ ...EMPTY_FILTER, tools: ['Bash'] }}
        seen={(f) => {
          current = f;
        }}
      />,
    );
    // 1) q 타이핑 → 300ms 타이머 예약(이 시점 filter.tools=['Bash'] 캡처).
    fireEvent.change(screen.getByPlaceholderText(/텍스트 검색|search text/), { target: { value: 'oom' } });
    // 2) 만료 전(150ms 경과) Bash 칩 제거 → onChange로 filter.tools=[] 반영.
    await act(() => vi.advanceTimersByTime(150));
    fireEvent.click(screen.getByRole('button', { name: /Bash/ }));
    expect(current.tools).toEqual([]);
    // 3) 타이머 만료 → debounce 콜백 발화.
    await act(() => vi.advanceTimersByTime(200));
    // 토글(Bash 제거)이 보존되고 q가 얹혀야 한다 — 되돌려지면 안 됨.
    expect(current.tools).toEqual([]);
    expect(current.q).toBe('oom');
    vi.useRealTimers();
  });

  it('collapses multiple keystrokes into one debounced onChange', async () => {
    vi.useFakeTimers();
    const onChange = vi.fn();
    render(<FilterBar filter={EMPTY_FILTER} onChange={onChange} matchedCount={null} notice={null} />);
    const input = screen.getByPlaceholderText(/텍스트 검색|search text/);
    fireEvent.change(input, { target: { value: 'o' } });
    fireEvent.change(input, { target: { value: 'oo' } });
    fireEvent.change(input, { target: { value: 'oom' } });
    await act(() => vi.advanceTimersByTime(320));
    expect(onChange).toHaveBeenCalledTimes(1);
    expect(onChange).toHaveBeenCalledWith(expect.objectContaining({ q: 'oom' }));
    vi.useRealTimers();
  });

  it('shows the jump-clear notice when provided', () => {
    render(
      <FilterBar
        filter={EMPTY_FILTER}
        onChange={() => {}}
        matchedCount={null}
        notice="이벤트로 이동하며 필터를 해제했습니다"
      />,
    );
    expect(screen.getByRole('status')).toHaveTextContent('필터를 해제했습니다');
  });
});

// 필터 값 발견성 (2026-07-05 사용자 피드백: "도구/모델에 뭘 써야 하는지 알 수
// 없다", "MCP 도구를 모아보고 싶은데 필터를 걸 방법이 마땅치 않다").
// 세션에 등장한 도구·모델을 토글 목록으로 제시하고, MCP 도구는 서버별 그룹
// 원클릭 토글(축 내 CSV OR)을 제공한다.
describe('FilterBar — 도구·모델 후보 목록', () => {
  const tools = ['Bash', 'Edit', 'mcp__plugin_serena_serena__find_symbol', 'mcp__plugin_serena_serena__read_file', 'mcp__github__get_me'];
  const models = ['claude-opus-4-8', 'claude-sonnet-5'];

  it('세션 등장 도구가 토글 버튼으로 나열되고 클릭 시 tools에 추가된다', () => {
    const onChange = vi.fn();
    render(
      <FilterBar filter={EMPTY_FILTER} onChange={onChange} matchedCount={null} notice={null}
        availableTools={tools} availableModels={models} />,
    );
    fireEvent.click(screen.getByRole('button', { name: 'Bash' }));
    expect(onChange).toHaveBeenCalledWith(expect.objectContaining({ tools: ['Bash'] }));
  });

  it('MCP 서버 그룹 토글 하나로 그 서버의 도구 전체가 tools에 들어간다', () => {
    const onChange = vi.fn();
    render(
      <FilterBar filter={EMPTY_FILTER} onChange={onChange} matchedCount={null} notice={null}
        availableTools={tools} availableModels={models} />,
    );
    fireEvent.click(screen.getByTestId('mcp-group-serena'));
    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({
        tools: ['mcp__plugin_serena_serena__find_symbol', 'mcp__plugin_serena_serena__read_file'],
      }),
    );
  });

  it('그룹이 이미 전부 선택돼 있으면 그룹 토글이 그 서버 도구를 전부 제거한다', () => {
    const onChange = vi.fn();
    const active = {
      ...EMPTY_FILTER,
      tools: ['Bash', 'mcp__plugin_serena_serena__find_symbol', 'mcp__plugin_serena_serena__read_file'],
    };
    render(
      <FilterBar filter={active} onChange={onChange} matchedCount={null} notice={null}
        availableTools={tools} availableModels={models} />,
    );
    fireEvent.click(screen.getByTestId('mcp-group-serena'));
    expect(onChange).toHaveBeenCalledWith(expect.objectContaining({ tools: ['Bash'] }));
  });

  it('관측 모델이 토글 버튼으로 나열되고 클릭 시 models에 추가된다', () => {
    const onChange = vi.fn();
    render(
      <FilterBar filter={EMPTY_FILTER} onChange={onChange} matchedCount={null} notice={null}
        availableTools={tools} availableModels={models} />,
    );
    fireEvent.click(screen.getByRole('button', { name: 'claude-sonnet-5' }));
    expect(onChange).toHaveBeenCalledWith(expect.objectContaining({ models: ['claude-sonnet-5'] }));
  });
});
