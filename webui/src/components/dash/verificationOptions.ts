/** 검증 탭 ECharts 옵션 빌더(순수) — kind×결과 100% 스택과 가드 행방
 *  Sankey(스펙 §3). 노드 라벨은 i18n에서 받아 `{label} {n}`으로 조립한다. */
import type { EChartsCoreOption } from 'echarts/core';
import type { VerificationSummaryDto } from '../../api/types';
import { AXIS_LABEL, OUTCOME_COLORS, SPLIT_LINE, TOOLTIP } from './echartsBase';

const MONO = 'ui-monospace,Menlo,monospace';

export function buildKindOption(
  sum: VerificationSummaryDto,
  labels: { passed: string; failed: string; unknown: string; notExec: string },
): EChartsCoreOption {
  // 가로 스택은 아래→위 순서라 보기 좋게 뒤집는다(총수 오름차순).
  const kinds = [...sum.by_kind].reverse();
  const totals = kinds.map((k) => k.passed + k.failed + k.unknown + k.not_executed);
  const pct = (v: number, i: number) => (totals[i] > 0 ? (v / totals[i]) * 100 : 0);
  const mk = (
    name: string,
    color: string,
    get: (k: VerificationSummaryDto['by_kind'][number]) => number,
    last = false,
  ) => ({
    name,
    type: 'bar',
    stack: 'k',
    barWidth: 17,
    data: kinds.map((k, i) => pct(get(k), i)),
    itemStyle: { color, opacity: 0.9, ...(last ? { borderRadius: [0, 3, 3, 0] } : {}) },
    ...(last
      ? {
          label: {
            show: true,
            position: 'right',
            distance: 8,
            color: '#6a7180',
            fontFamily: MONO,
            fontSize: 10.5,
            formatter: (p: { dataIndex: number }) => String(totals[p.dataIndex]),
          },
        }
      : {}),
  });
  return {
    animationDuration: 700,
    grid: { left: 86, right: 64, top: 8, bottom: 24 },
    tooltip: {
      ...TOOLTIP,
      formatter: (p: { name: string }) => {
        const k = sum.by_kind.find((x) => x.kind === p.name);
        if (!k) return p.name;
        const t = k.passed + k.failed + k.unknown + k.not_executed;
        return (
          `<b style="font-family:${MONO}">${k.kind}</b> · ${t}<br>` +
          `<span style="color:${OUTCOME_COLORS.passed}">${labels.passed} ${k.passed}</span> · ` +
          `<span style="color:${OUTCOME_COLORS.failed}">${labels.failed} ${k.failed}</span> · ` +
          `<span style="color:#6a7180">${labels.unknown} ${k.unknown}</span> · ` +
          `<span style="color:#6a7180">${labels.notExec} ${k.not_executed}</span>`
        );
      },
    },
    xAxis: {
      type: 'value',
      max: 100,
      axisLabel: { ...AXIS_LABEL, formatter: (v: number) => `${v}%` },
      splitLine: SPLIT_LINE,
    },
    yAxis: {
      type: 'category',
      data: kinds.map((k) => k.kind),
      axisLine: { show: false },
      axisTick: { show: false },
      axisLabel: AXIS_LABEL,
    },
    series: [
      mk(labels.passed, OUTCOME_COLORS.passed, (k) => k.passed),
      mk(labels.failed, OUTCOME_COLORS.failed, (k) => k.failed),
      mk(labels.unknown, OUTCOME_COLORS.unknown, (k) => k.unknown),
      mk(labels.notExec, OUTCOME_COLORS.not_executed, (k) => k.not_executed, true),
    ],
  };
}

export type SankeyLabels = {
  guards: string;
  measured: string;
  unknown: string;
  notExec: string;
  passed: string;
  failed: string;
  recovered: string;
  abandoned: string;
  piped: string;
  other: string;
};

export function buildSankeyOption(
  sum: VerificationSummaryDto,
  L: SankeyLabels,
): EChartsCoreOption {
  const n = (label: string, v: number) => `${label} ${v}`;
  const N = {
    guards: n(L.guards, sum.total),
    measured: n(L.measured, sum.measured),
    unknown: n(L.unknown, sum.unknown),
    notExec: n(L.notExec, sum.not_executed),
    passed: n(L.passed, sum.passed),
    failed: n(L.failed, sum.failed),
    recovered: n(L.recovered, sum.failures.recovered),
    abandoned: n(L.abandoned, sum.failures.abandoned),
    piped: n(L.piped, sum.unknown_piped),
    other: n(L.other, sum.unknown_other),
  };
  const data = [
    { name: N.guards, itemStyle: { color: '#7da7ff' } },
    { name: N.measured, itemStyle: { color: '#5f86c9' } },
    { name: N.unknown, itemStyle: { color: '#4a5162' } },
    { name: N.passed, itemStyle: { color: OUTCOME_COLORS.passed } },
    { name: N.failed, itemStyle: { color: OUTCOME_COLORS.failed } },
    { name: N.recovered, itemStyle: { color: '#2f9668' } },
    { name: N.abandoned, itemStyle: { color: '#ff6b6b' } },
    { name: N.piped, itemStyle: { color: '#5a6172' } },
    { name: N.other, itemStyle: { color: '#3d4351' } },
  ];
  const links = [
    { source: N.guards, target: N.measured, value: sum.measured },
    { source: N.guards, target: N.unknown, value: sum.unknown },
    { source: N.measured, target: N.passed, value: sum.passed },
    { source: N.measured, target: N.failed, value: sum.failed },
    { source: N.failed, target: N.recovered, value: sum.failures.recovered },
    { source: N.failed, target: N.abandoned, value: sum.failures.abandoned },
    { source: N.unknown, target: N.piped, value: sum.unknown_piped },
    { source: N.unknown, target: N.other, value: sum.unknown_other },
  ].filter((l) => l.value > 0);
  if (sum.not_executed > 0) {
    data.push({ name: N.notExec, itemStyle: { color: OUTCOME_COLORS.not_executed } });
    links.push({ source: N.guards, target: N.notExec, value: sum.not_executed });
  }
  return {
    animationDuration: 800,
    tooltip: {
      ...TOOLTIP,
      trigger: 'item',
      formatter: (p: { dataType?: string; data?: { source?: string; target?: string; value?: number }; name?: string }) =>
        p.dataType === 'edge'
          ? `${p.data?.source} → ${p.data?.target}<br><b style="font-family:${MONO}">${p.data?.value}</b>`
          : `<b>${p.name}</b>`,
    },
    series: [
      {
        type: 'sankey',
        left: 8,
        right: 130,
        top: 14,
        bottom: 10,
        nodeWidth: 10,
        nodeGap: 18,
        draggable: false,
        label: { color: '#aab0bd', fontSize: 11, fontFamily: MONO },
        lineStyle: { color: 'gradient', opacity: 0.28, curveness: 0.55 },
        itemStyle: { borderWidth: 0 },
        emphasis: { focus: 'adjacency' },
        data,
        links,
      },
    ],
  };
}
