/** 세션 분포 스캐터 옵션 빌더(스펙 §1 모듈 6) — x 과금 토큰(log, 0 제외),
 *  y 신호/100 events, 크기 √비용, 색 주 모델(최초 관측순 SSOT), 중앙값
 *  점선, 이상점(비용 상위 2 ∪ 밀도 상위 2)만 직접 라벨. 순수 함수. */
import type { EChartsCoreOption } from 'echarts/core';
import type { SessionSeriesRowDto } from '../../api/types';
import { modelColors, signalsOf, MODEL_OVERFLOW_COLOR } from '../../lib/dashDerive';
import { cohortModels, displayModel, usageRatios } from '../../lib/seriesView';
import { AXIS_LABEL, SPLIT_LINE, TOOLTIP } from './echartsBase';

const MONO = 'ui-monospace,Menlo,monospace';

type Pt = {
  name: string;
  value: [number, number, number];
  labeled: boolean;
  s: {
    sid: string;
    date: string;
    models: string[];
    billedM: number;
    cost: number;
    rate: number | null;
    signals: number;
    events: number;
    hit: number | null;
  };
};

function median(vals: number[]): number {
  if (vals.length === 0) return 0;
  const s = [...vals].sort((a, b) => a - b);
  return s[Math.floor(s.length / 2)];
}

export function buildScatterOption(args: {
  rows: SessionSeriesRowDto[];
  nameOf: (sid: string) => string;
  labels: { x: string; y: string; unassigned: string; click: string };
}): { option: EChartsCoreOption; points: number } {
  const { rows, nameOf, labels } = args;
  const colors = modelColors(rows);
  const pts: Array<{ prim: string | null; p: Pt }> = [];
  for (const r of rows) {
    const m = r.metrics;
    const billed =
      (m.input_tokens ?? 0) + (m.cache_creation_input_tokens ?? 0) + (m.output_tokens ?? 0);
    if (billed <= 0) continue; // log 축 — 미측정/0 과금 세션은 놓을 곳이 없다
    const ratios = usageRatios(m);
    const sig = signalsOf(m);
    const dens = r.event_count > 0 ? (sig / r.event_count) * 100 : 0;
    const models = cohortModels(r.fingerprint);
    pts.push({
      prim: models[0] ?? null,
      p: {
        name: nameOf(r.session_id),
        value: [billed / 1e6, Math.round(dens * 100) / 100, m.estimated_cost_usd ?? 0],
        labeled: false,
        s: {
          sid: r.session_id,
          date: r.first_observed_at.slice(5, 10),
          models: models.map(displayModel),
          billedM: Math.round((billed / 1e6) * 10) / 10,
          cost: Math.round(m.estimated_cost_usd ?? 0),
          rate: ratios.measured ? ratios.unitRatePerM : null,
          signals: sig,
          events: r.event_count,
          hit: ratios.measured ? ratios.cacheHitPct : null,
        },
      },
    });
  }
  // 이상점 라벨: 비용 상위 2 ∪ 신호 밀도 상위 2 (결정론).
  const byCost = [...pts].sort((a, b) => b.p.value[2] - a.p.value[2]).slice(0, 2);
  const byDens = [...pts].sort((a, b) => b.p.value[1] - a.p.value[1]).slice(0, 2);
  for (const o of [...byCost, ...byDens]) o.p.labeled = true;

  const medX = median(pts.map(({ p }) => p.value[0]));
  const medY = median(pts.map(({ p }) => p.value[1]));

  // 계열 = 주 모델(최초 관측순) — 색이 모델 정체성을 따른다.
  const groups = new Map<string | null, Pt[]>();
  for (const { prim, p } of pts) {
    if (!groups.has(prim)) groups.set(prim, []);
    groups.get(prim)!.push(p);
  }
  const order = [...colors.keys()].filter((m) => groups.has(m));
  if (groups.has(null)) order.push(null as unknown as string);

  const option: EChartsCoreOption = {
    animationDuration: 800,
    grid: { left: 56, right: 110, top: 34, bottom: 44 },
    legend: {
      top: 0,
      right: 0,
      textStyle: { color: '#aab0bd', fontSize: 11 },
      itemWidth: 10,
      itemHeight: 10,
      icon: 'circle',
    },
    tooltip: {
      ...TOOLTIP,
      formatter: (p: { data: Pt }) => {
        const s = p.data.s;
        return (
          `<div style="font-family:${MONO};font-weight:650;margin-bottom:6px">${p.data.name}</div>` +
          `<div style="color:#aab0bd;font-size:11.5px;line-height:1.75">` +
          `${s.date} · ${s.models.join(' + ') || labels.unassigned}<br>` +
          `<b style="color:#e6e8ee">${s.billedM}M</b> · <b style="color:#e6e8ee">$${s.cost}</b>` +
          `${s.rate !== null ? ` · <b style="color:#e6e8ee">$${s.rate}/1M</b>` : ''}<br>` +
          `${s.signals} / ${s.events.toLocaleString()} ev` +
          `${s.hit !== null ? ` · <b style="color:#e6e8ee">${s.hit}%</b>` : ''}</div>` +
          `<div style="color:#6a7180;font-size:10.5px;margin-top:5px">${labels.click}</div>`
        );
      },
    },
    xAxis: {
      type: 'log',
      name: labels.x,
      nameLocation: 'middle',
      nameGap: 30,
      nameTextStyle: { color: '#6a7180', fontSize: 10.5 },
      axisLine: { show: false },
      axisTick: { show: false },
      axisLabel: { ...AXIS_LABEL, formatter: (v: number) => `${v}M` },
      splitLine: SPLIT_LINE,
    },
    yAxis: {
      type: 'value',
      name: labels.y,
      nameTextStyle: { color: '#6a7180', fontSize: 10.5, align: 'left' },
      axisLine: { show: false },
      axisTick: { show: false },
      axisLabel: AXIS_LABEL,
      splitLine: SPLIT_LINE,
    },
    series: order.map((key) => ({
      name: key ? displayModel(key) : labels.unassigned,
      type: 'scatter',
      data: groups.get(key)!,
      symbolSize: (d: [number, number, number]) => 6 + Math.sqrt(Math.max(0, d[2])) * 1.15,
      itemStyle: {
        color: key ? (colors.get(key) ?? MODEL_OVERFLOW_COLOR) : MODEL_OVERFLOW_COLOR,
        opacity: 0.85,
        borderColor: '#0b0d12',
        borderWidth: 1,
      },
      emphasis: { scale: 1.4 },
      labelLayout: { hideOverlap: true },
      label: {
        show: true,
        position: 'right',
        distance: 7,
        color: '#6a7180',
        fontSize: 10,
        fontFamily: MONO,
        formatter: (p: { data: Pt }) => (p.data.labeled ? p.data.name.slice(0, 22) : ''),
      },
      markLine: {
        silent: true,
        symbol: 'none',
        lineStyle: { color: '#2a3040', type: 'dashed' },
        label: { show: false },
        data: [{ xAxis: medX }, { yAxis: medY }],
      },
    })),
  };
  return { option, points: pts.length };
}
