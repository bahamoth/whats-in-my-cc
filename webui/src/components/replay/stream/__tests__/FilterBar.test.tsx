// FilterBar — Task 10 (스펙 §1.4). Controlled component: 축별 칩 드롭다운 +
// 텍스트 검색(디바운스) + 활성 조건 제거형 칩 + "N건 매칭" + 점프-해제 알림.
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { act, fireEvent, screen } from '@testing-library/react';
import { renderWithI18n as render } from '../../../../test/i18nRender';
import '@testing-library/jest-dom/vitest';
import { FilterBar } from '../FilterBar';
import { EMPTY_FILTER } from '../filterState';

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
