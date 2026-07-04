/** 코호트 비교 슬로프 — 결정론 라벨(i18n 조립)·표본 부족 가드·동시 CC 변경
 *  각주·null 미렌더를 잠근다. */
import { render, screen, cleanup } from '@testing-library/react';
import { describe, expect, it, vi, afterEach, beforeAll } from 'vitest';
import { CohortCompareCards } from '../CohortCompare';
import { I18nProvider } from '../../../i18n';
import type { CohortCompare } from '../../../lib/dashDerive';

vi.mock('../EChart', () => ({
  EChart: ({ height }: { height: number }) => <div data-testid="echart" style={{ height }} />,
}));

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

const base: CohortCompare = {
  added: ['Fable 5'],
  removed: [],
  boundaryIdx: 3,
  alsoCcChanged: false,
  lowSample: false,
  before: { n: 3, unitRatePerM: 10.7, passRatePct: 63, signalsPerSession: 6.8, cacheHitPct: 96.5 },
  after: { n: 13, unitRatePerM: 14.1, passRatePct: 71, signalsPerSession: 7.4, cacheHitPct: 95.2 },
};

function mount(c: CohortCompare | null) {
  return render(
    <I18nProvider initialLocale="en">
      <CohortCompareCards c={c} />
    </I18nProvider>,
  );
}

describe('CohortCompareCards', () => {
  it('경계 라벨과 전/후 n을 렌더', () => {
    mount(base);
    expect(screen.getByText(/cohort compare/i)).toHaveTextContent('Fable 5 introduced');
    expect(screen.getByText(/before 3 · after 13 sessions/i)).toBeInTheDocument();
    expect(screen.getAllByTestId('echart')).toHaveLength(4);
  });
  it('표본 부족이면 delta 칩 대신 배지', () => {
    mount({ ...base, lowSample: true });
    expect(screen.getAllByText(/low sample/i).length).toBeGreaterThan(0);
  });
  it('동시 CC 변경 각주', () => {
    mount({ ...base, alsoCcChanged: true });
    expect(screen.getByText(/claude code version also changed/i)).toBeInTheDocument();
  });
  it('경계 없으면 미렌더', () => {
    const { container } = mount(null);
    expect(container.textContent).toBe('');
  });
});
