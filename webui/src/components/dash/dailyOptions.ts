/**
 * 일별 검증/비용·신호 차트의 ECharts 옵션 빌더 — 순수 함수(테스트 SSOT:
 * __tests__/dailyOptions.test.ts). 렌더는 <EChart>가, 파생은 lib/dashDerive가,
 * 문구는 i18n 카탈로그(labels 인자)가 책임진다.
 */
import type { EChartsCoreOption } from 'echarts/core';
import type { Daily } from '../../lib/dashDerive';
import { AXIS_LABEL, OUTCOME_COLORS, SPLIT_LINE, TOOLTIP, rampColor } from './echartsBase';

export type CohortMarker = {
  dayIdx: number;
  label: string;
  /** 코호트 섹션에서 선택된 경계 — 진하게, 나머지 유의 경계는 흐리게. */
  emphasis?: boolean;
};
export type DayDetail = {
  name: string;
  cost: number;
  passed: number;
  guards: number;
  signals: number;
};

const MONO = 'ui-monospace,Menlo,monospace';

function markLine(markers: CohortMarker[], nDays: number) {
  return {
    symbol: 'none',
    animationDuration: 900,
    lineStyle: { color: '#b07dff', type: 'dashed', opacity: 0.75 },
    label: {
      color: '#b07dff',
      fontFamily: MONO,
      fontSize: 10.5,
      formatter: (p: { name: string }) => p.name,
    },
    // 우측 40% 구간의 마커는 라벨을 선 왼쪽으로 — 차트 밖 클리핑 방지.
    // 선택 경계(emphasis)는 진하게, 나머지는 흐리게 — 코호트 섹션과 동기.
    data: markers.map((m) => ({
      name: m.label,
      xAxis: m.dayIdx,
      lineStyle: { opacity: m.emphasis ? 0.95 : 0.35 },
      label: {
        ...(m.dayIdx / Math.max(1, nDays - 1) > 0.6
          ? { align: 'right' as const, padding: [0, 6, 0, 0] }
          : { align: 'left' as const, padding: [0, 0, 0, 6] }),
        opacity: m.emphasis ? 1 : 0.55,
        fontWeight: m.emphasis ? 700 : 400,
      },
    })),
  };
}

const rowLine = (left: string, right: string) =>
  `<div style="display:flex;justify-content:space-between;gap:18px;color:#aab0bd">` +
  `<span style="font-family:${MONO}">${left}</span>` +
  `<b style="font-family:${MONO};color:#e6e8ee">${right}</b></div>`;

export function buildVerOption(args: {
  daily: Daily;
  markers: CohortMarker[];
  details: DayDetail[][];
  labels: { passed: string; failed: string; unknown: string; noGuards: string };
}): EChartsCoreOption {
  const { daily, markers, details, labels } = args;
  return {
    animationDuration: 700,
    animationEasing: 'cubicOut',
    grid: { left: 56, right: 18, top: 26, bottom: 30 },
    tooltip: {
      ...TOOLTIP,
      trigger: 'axis',
      axisPointer: { type: 'shadow' },
      formatter: (ps: Array<{ dataIndex: number }>) => {
        const i = ps[0].dataIndex;
        const ss = (details[i] ?? []).filter((d) => d.guards > 0);
        const rows = ss.length
          ? ss.map((d) => rowLine(d.name.slice(0, 24), `${d.passed}/${d.guards}`)).join('')
          : `<div style="color:#6a7180">${labels.noGuards}</div>`;
        return (
          `<div style="font-family:${MONO};font-weight:650;margin-bottom:6px">${daily.dates[i]}` +
          `<span style="color:${OUTCOME_COLORS.passed};margin-left:8px">${labels.passed} ${daily.passed[i]}</span>` +
          `<span style="color:${OUTCOME_COLORS.failed};margin-left:8px">${labels.failed} ${daily.failed[i]}</span>` +
          `<span style="color:#6a7180;margin-left:8px">${labels.unknown} ${daily.unknown[i]}</span></div>` +
          rows
        );
      },
    },
    xAxis: {
      type: 'category',
      data: daily.dates,
      axisLine: { show: false },
      axisTick: { show: false },
      axisLabel: { ...AXIS_LABEL, interval: Math.max(0, Math.ceil(daily.dates.length / 8) - 1) },
      splitLine: { show: false },
    },
    yAxis: { type: 'value', axisLabel: AXIS_LABEL, splitLine: SPLIT_LINE },
    series: [
      {
        name: labels.passed,
        type: 'bar',
        stack: 'v',
        data: daily.passed,
        barMaxWidth: 26,
        itemStyle: { color: OUTCOME_COLORS.passed, opacity: 0.92 },
      },
      {
        name: labels.failed,
        type: 'bar',
        stack: 'v',
        data: daily.failed,
        barMaxWidth: 26,
        itemStyle: { color: OUTCOME_COLORS.failed, opacity: 0.92 },
      },
      {
        name: labels.unknown,
        type: 'bar',
        stack: 'v',
        data: daily.unknown,
        barMaxWidth: 26,
        itemStyle: { color: OUTCOME_COLORS.unknown, opacity: 0.75, borderRadius: [3, 3, 0, 0] },
        markLine: markLine(markers, daily.dates.length),
      },
    ],
  };
}

export function buildCostOption(args: {
  daily: Daily;
  markers: CohortMarker[];
  details: DayDetail[][];
  labels: { signals: string; noSessions: string };
}): EChartsCoreOption {
  const { daily, markers, details, labels } = args;
  const sigMax = Math.max(...daily.signals, 1);
  return {
    animationDuration: 700,
    animationEasing: 'cubicOut',
    grid: { left: 56, right: 18, top: 26, bottom: 30 },
    tooltip: {
      ...TOOLTIP,
      trigger: 'axis',
      axisPointer: { type: 'shadow' },
      formatter: (ps: Array<{ dataIndex: number }>) => {
        const i = ps[0].dataIndex;
        const ss = details[i] ?? [];
        const rows = ss.length
          ? ss.map((d) => rowLine(d.name.slice(0, 24), `$${d.cost}`)).join('')
          : `<div style="color:#6a7180">${labels.noSessions}</div>`;
        return (
          `<div style="font-family:${MONO};font-weight:650;margin-bottom:6px">${daily.dates[i]}` +
          `<span style="color:#41c285;margin-left:8px">$${Math.round(daily.cost[i])}</span>` +
          `<span style="color:#6a7180;margin-left:8px">${labels.signals} ${daily.signals[i]}</span></div>` +
          rows
        );
      },
    },
    xAxis: {
      type: 'category',
      data: daily.dates,
      axisLine: { show: false },
      axisTick: { show: false },
      axisLabel: { ...AXIS_LABEL, interval: Math.max(0, Math.ceil(daily.dates.length / 8) - 1) },
      splitLine: { show: false },
    },
    yAxis: {
      type: 'value',
      axisLabel: { ...AXIS_LABEL, formatter: (v: number) => `$${v}` },
      splitLine: SPLIT_LINE,
    },
    series: [
      {
        type: 'bar',
        data: daily.cost.map((v, i) => ({
          value: v,
          itemStyle: {
            color: rampColor(daily.signals[i] / sigMax),
            borderRadius: [3, 3, 0, 0],
            opacity: 0.93,
          },
        })),
        barMaxWidth: 26,
        markLine: markLine(markers, daily.dates.length),
      },
    ],
  };
}
