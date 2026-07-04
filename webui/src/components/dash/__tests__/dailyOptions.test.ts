/**
 * 일별 검증/비용·신호 ECharts 옵션 빌더 — 순수 함수. 스택 구성·outcome 색·
 * 코호트 markLine·신호 그라데이션(막대 색 = rampColor(신호/최대))을 잠근다.
 */
import { describe, expect, it } from 'vitest';
import { buildVerOption, buildCostOption, type DayDetail } from '../dailyOptions';
import { OUTCOME_COLORS, rampColor } from '../echartsBase';
import type { Daily } from '../../../lib/dashDerive';

const daily: Daily = {
  dates: ['06-05', '06-06', '06-07'],
  cost: [100, 0, 40],
  signals: [8, 0, 2],
  passed: [5, 0, 3],
  failed: [1, 0, 0],
  unknown: [2, 0, 1],
  sessionsOf: [[0], [], [1]],
};
const details: DayDetail[][] = [
  [{ name: 'aurora', cost: 100, passed: 5, guards: 8, signals: 8 }],
  [],
  [{ name: 'fern', cost: 40, passed: 3, guards: 4, signals: 2 }],
];
const markers = [{ dayIdx: 2, label: 'Fable 5 첫 관측' }];
const verLabels = { passed: 'passed', failed: 'failed', unknown: 'unknown', noGuards: 'no guards' };
const costLabels = { signals: 'signals', noSessions: 'no sessions' };

describe('buildVerOption', () => {
  const o = buildVerOption({ daily, markers, details, labels: verLabels }) as Record<string, any>;
  it('통과/실패/판정불가 3계열 스택 + outcome 색', () => {
    expect(o.series).toHaveLength(3);
    expect(o.series.map((s: any) => s.stack)).toEqual(['v', 'v', 'v']);
    expect(o.series[0].itemStyle.color).toBe(OUTCOME_COLORS.passed);
    expect(o.series[1].itemStyle.color).toBe(OUTCOME_COLORS.failed);
    expect(o.series[2].itemStyle.color).toBe(OUTCOME_COLORS.unknown);
    expect(o.series[0].data).toEqual(daily.passed);
  });
  it('x축 = 일자, 코호트 markLine 라벨 보존', () => {
    expect(o.xAxis.data).toEqual(daily.dates);
    const ml = o.series[2].markLine.data;
    expect(ml[0].name).toBe('Fable 5 첫 관측');
    expect(ml[0].xAxis).toBe(2);
    // dayIdx 2/2 = 우측 60% 초과 — 라벨은 선 왼쪽 정렬(클리핑 방지)
    expect(ml[0].label.align).toBe('right');
  });
});

describe('buildCostOption', () => {
  const o = buildCostOption({ daily, markers, details, labels: costLabels }) as Record<string, any>;
  it('막대 색 = 그날 신호 수의 램프값(최대 대비)', () => {
    const bars = o.series[0].data;
    expect(bars[0].itemStyle.color).toBe(rampColor(8 / 8));
    expect(bars[1].itemStyle.color).toBe(rampColor(0));
    expect(bars[2].itemStyle.color).toBe(rampColor(2 / 8));
    expect(bars[0].value).toBe(100);
  });
  it('차트 내 레인지 컨트롤(dataZoom)은 없다 — 기간은 상단 컨트롤 전담', () => {
    expect(o.dataZoom).toBeUndefined();
    expect(o.series[0].markLine.data[0].name).toBe('Fable 5 첫 관측');
  });
});
