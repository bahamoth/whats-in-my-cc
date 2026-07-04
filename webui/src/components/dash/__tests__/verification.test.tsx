/** 검증 탭 — Sankey 보존(합계 일치·미실행 분기)·kind 100% 정규화·리듬 dot
 *  수·커버리지 %를 잠근다. */
import { render, screen, cleanup } from '@testing-library/react';
import { describe, expect, it, vi, afterEach } from 'vitest';
import { buildKindOption, buildSankeyOption } from '../verificationOptions';
import { GuardRhythm } from '../GuardRhythm';
import { ChangeCoverage } from '../ChangeCoverage';
import { I18nProvider } from '../../../i18n';
import type { VerificationSummaryDto } from '../../../api/types';

vi.mock('../EChart', () => ({
  EChart: () => <div data-testid="echart" />,
}));
afterEach(cleanup);

const SUM: VerificationSummaryDto = {
  total: 100,
  measured: 70,
  passed: 55,
  failed: 15,
  unknown: 20,
  unknown_piped: 14,
  unknown_other: 6,
  not_executed: 10,
  by_kind: [
    { kind: 'test', passed: 40, failed: 10, unknown: 10, not_executed: 0 },
    { kind: 'build', passed: 15, failed: 5, unknown: 10, not_executed: 10 },
  ],
  failures: { recovered: 11, abandoned: 4 },
  rhythm: [
    {
      session_id: 'sess_a',
      guards: 3,
      passed: 2,
      runs: [
        { pct: 25, status: 'failed' },
        { pct: 50, status: 'passed' },
        { pct: 75, status: 'passed' },
      ],
    },
  ],
  coverage: {
    covered: 30,
    total: 40,
    by_session: [{ session_id: 'sess_a', covered: 30, total: 40 }],
  },
};

const NODE_LABELS = {
  guards: 'guards',
  measured: 'measured',
  unknown: 'undecidable',
  notExec: 'not executed',
  passed: 'passed',
  failed: 'failed',
  recovered: 'recovered',
  abandoned: 'failed & left',
  piped: 'pipe-masked',
  other: 'other',
};

describe('buildSankeyOption', () => {
  const o = buildSankeyOption(SUM, NODE_LABELS) as Record<string, any>;
  const links = o.series[0].links as Array<{ source: string; target: string; value: number }>;
  it('가드 유출 합 = total, 미실행 분기 포함', () => {
    const outOfGuards = links
      .filter((l) => l.source.startsWith('guards'))
      .reduce((a, l) => a + l.value, 0);
    expect(outOfGuards).toBe(100);
    expect(links.some((l) => l.target.startsWith('not executed'))).toBe(true);
  });
  it('실패 분기 = 복구 + 방치, 판정불가 = 파이프 + 기타', () => {
    expect(links.find((l) => l.target.startsWith('recovered'))!.value).toBe(11);
    expect(links.find((l) => l.target.startsWith('failed & left'))!.value).toBe(4);
    expect(links.find((l) => l.target.startsWith('pipe-masked'))!.value).toBe(14);
  });
});

describe('buildKindOption', () => {
  const o = buildKindOption(SUM, { passed: 'p', failed: 'f', unknown: 'u', notExec: 'n' }) as Record<string, any>;
  it('kind별 100% 정규화 4계열 스택', () => {
    expect(o.series).toHaveLength(4);
    const idx = (o.yAxis.data as string[]).indexOf('build');
    const total = o.series.reduce((a: number, s: any) => a + s.data[idx], 0);
    expect(total).toBeCloseTo(100, 5);
  });
});

describe('GuardRhythm / ChangeCoverage', () => {
  it('리듬 dot 수 = runs 수, 커버리지 %와 미커버 수 렌더', () => {
    render(
      <I18nProvider initialLocale="en">
        <GuardRhythm rhythm={SUM.rhythm} nameOf={(s) => s} />
        <ChangeCoverage coverage={SUM.coverage} nameOf={(s) => s} />
      </I18nProvider>,
    );
    expect(document.querySelectorAll('[data-dot]')).toHaveLength(3);
    expect(screen.getByText(/overall 75%/)).toBeInTheDocument();
    expect(screen.getAllByText(/10/).length).toBeGreaterThan(0);
  });
});
