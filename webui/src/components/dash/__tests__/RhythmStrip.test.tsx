import { fireEvent, render } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import { describe, expect, it, vi } from 'vitest';
import { RhythmStrip } from '../RhythmStrip';

const runs = [
  { pct: 25, status: 'failed' },
  { pct: 75, status: 'passed' },
];

describe('RhythmStrip', () => {
  it('점 수 = runs 수, 위치는 pct, 색은 status(OUTCOME_COLORS)', () => {
    render(<RhythmStrip runs={runs} />);
    const dots = document.querySelectorAll('[data-dot]');
    expect(dots).toHaveLength(2);
    expect((dots[0] as HTMLElement).style.left).toBe('25%');
    // OUTCOME_COLORS.failed = #ef4747
    expect((dots[0] as HTMLElement).style.background).toBe('rgb(239, 71, 71)');
  });

  it('onRunClick가 있으면 점이 버튼이 되고 인덱스로 콜백한다', () => {
    const onClick = vi.fn();
    render(<RhythmStrip runs={runs} onRunClick={onClick} />);
    const dots = document.querySelectorAll('button[data-dot]');
    expect(dots).toHaveLength(2);
    fireEvent.click(dots[1]);
    expect(onClick).toHaveBeenCalledWith(1);
  });
});
