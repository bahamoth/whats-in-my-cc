/**
 * EChart 래퍼 lifecycle — echarts/core를 mock해 init/dispose/setOption 계약을
 * 잠근다(실캔버스는 jsdom에 없음). rampColor는 실함수로 구간 보간을 잠근다.
 */
import { render, cleanup } from '@testing-library/react';
import { describe, it, expect, vi, afterEach, beforeAll } from 'vitest';

const { init } = vi.hoisted(() => ({
  init: vi.fn(() => ({
    setOption: vi.fn(),
    resize: vi.fn(),
    dispose: vi.fn(),
    on: vi.fn(),
    group: undefined as string | undefined,
  })),
}));
vi.mock('echarts/core', async (importOriginal) => {
  const orig = await importOriginal<object>();
  return { ...orig, init, connect: vi.fn() };
});

import { EChart } from '../EChart';
import { rampColor } from '../echartsBase';

beforeAll(() => {
  vi.stubGlobal(
    'ResizeObserver',
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  );
});
afterEach(cleanup);

describe('EChart', () => {
  it('mount 시 echarts.init, unmount 시 dispose', () => {
    const { unmount } = render(<EChart option={{ series: [] }} height={100} />);
    expect(init).toHaveBeenCalledTimes(1);
    const inst = init.mock.results[0]!.value;
    unmount();
    expect(inst.dispose).toHaveBeenCalled();
  });

  it('option 변경 시 setOption(notMerge:true)', () => {
    const { rerender } = render(<EChart option={{ series: [] }} height={100} />);
    const inst = init.mock.results.at(-1)!.value;
    rerender(<EChart option={{ series: [{}] }} height={100} />);
    expect(inst.setOption).toHaveBeenLastCalledWith({ series: [{}] }, { notMerge: true });
  });
});

describe('rampColor', () => {
  it('끝점은 램프의 정의 색', () => {
    expect(rampColor(0)).toBe('#41c285');
    expect(rampColor(1)).toBe('#ef6047');
  });
  it('범위 밖은 클램프, 중간값은 hex 형식', () => {
    expect(rampColor(-1)).toBe('#41c285');
    expect(rampColor(2)).toBe('#ef6047');
    expect(rampColor(0.5)).toMatch(/^#[0-9a-f]{6}$/);
  });
});
