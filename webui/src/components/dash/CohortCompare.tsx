/** 코호트 비교 — 최신 모델-집합 경계의 인접 세그먼트 전/후 슬로프 4종
 *  (단가·통과율·신호/세션·캐시 적중). 변화의 "방향"이 본질이라 슬로프가
 *  형태다(스펙 §1 모듈 4·§2). 표본 부족이면 delta 강조를 끈다. */
import type { EChartsCoreOption } from 'echarts/core';
import type { CohortCompare } from '../../lib/dashDerive';
import { EChart } from './EChart';
import { useT } from '../../i18n';

const trim1 = (v: number) => String(Math.round(v * 10) / 10);

/** 2점 슬로프 옵션 — worse(나쁜 방향)면 앰버, 아니면 그린. 순수 함수. */
export function buildSlopeOption(
  before: number,
  after: number,
  fmt: (v: number) => string,
  worse: boolean,
  axisLabels: [string, string],
): EChartsCoreOption {
  const col = worse ? '#f0b429' : '#41c285';
  return {
    animationDuration: 800,
    grid: { left: 10, right: 10, top: 16, bottom: 20 },
    xAxis: {
      type: 'category',
      data: axisLabels,
      axisLine: { show: false },
      axisTick: { show: false },
      axisLabel: { color: '#6a7180', fontSize: 10.5 },
    },
    yAxis: {
      type: 'value',
      show: false,
      min: Math.min(before, after) * 0.9,
      max: Math.max(before, after) * 1.08 || 1,
    },
    series: [
      {
        type: 'line',
        data: [before, after],
        symbolSize: 7,
        lineStyle: { width: 2.5, color: col },
        itemStyle: { color: col, borderColor: '#0b0d12', borderWidth: 2 },
        label: {
          show: true,
          color: '#aab0bd',
          fontFamily: 'ui-monospace,Menlo,monospace',
          fontSize: 10.5,
          position: 'top',
          formatter: (p: { value: number }) => fmt(p.value),
        },
      },
    ],
  };
}

type MetricDef = {
  name: string;
  before: number | null;
  after: number | null;
  fmt: (v: number) => string;
  /** 증가가 나쁜 방향인가 (단가·신호/세션 = true). */
  upIsWorse: boolean;
  deltaUnit: string;
};

function SlopeCard({ m, lowSample, lowSampleLabel, axisLabels }: {
  m: MetricDef;
  lowSample: boolean;
  lowSampleLabel: string;
  axisLabels: [string, string];
}) {
  const has = m.before !== null && m.after !== null;
  const d = has ? Math.round(((m.after as number) - (m.before as number)) * 100) / 100 : null;
  const worse = d !== null && (d > 0 ? m.upIsWorse : d < 0 ? !m.upIsWorse : false);
  const chip =
    d === null || lowSample ? null : (
      <span
        className={`ml-2 rounded-[5px] px-1.5 py-0.5 align-[3px] font-mono text-[11px] ${
          Math.abs(d) < 0.05
            ? 'bg-(--wimcc-surface-2) text-(--wimcc-fg-subtle)'
            : worse
              ? 'bg-[#f0b429]/10 text-[#f0b429]'
              : 'bg-[#41c285]/10 text-[#41c285]'
        }`}
      >
        {Math.abs(d) < 0.05 ? '▬ 0.0' : `${d > 0 ? '▲' : '▼'} ${trim1(Math.abs(d))}${m.deltaUnit}`}
      </span>
    );
  return (
    <div className="rounded-[13px] border border-(--wimcc-border) bg-(--wimcc-surface-1) px-4 pt-3.5 pb-1">
      <span className="text-[11.5px] font-semibold text-(--wimcc-fg-muted)">{m.name}</span>
      <div className="my-1 font-mono text-[24px] font-semibold">
        {m.after !== null ? m.fmt(m.after) : '—'}
        {chip}
        {lowSample && (
          <span className="ml-2 rounded-[5px] bg-(--wimcc-surface-2) px-1.5 py-0.5 align-[3px] font-mono text-[11px] text-(--wimcc-fg-subtle)">
            {lowSampleLabel}
          </span>
        )}
      </div>
      <div className="font-mono text-[11px] text-(--wimcc-fg-subtle)">
        {m.before !== null ? m.fmt(m.before) : '—'} → {m.after !== null ? m.fmt(m.after) : '—'}
      </div>
      {has ? (
        <EChart
          option={buildSlopeOption(m.before as number, m.after as number, m.fmt, worse, axisLabels)}
          height={74}
        />
      ) : (
        <div style={{ height: 74 }} />
      )}
    </div>
  );
}

export function CohortCompareCards({ c }: { c: CohortCompare | null }) {
  const t = useT();
  if (!c) return null;
  const label =
    c.added.length && c.removed.length
      ? `${c.removed.join(' · ')} → ${c.added.join(' · ')}`
      : c.added.length
        ? t('dash.cohort.introduced', c.added.join(' · '))
        : t('dash.cohort.retired', c.removed.join(' · '));
  const axisLabels: [string, string] = [t('dash.cohort.before'), t('dash.cohort.after')];
  const metrics: MetricDef[] = [
    {
      name: t('dash.head.rate'),
      before: c.before.unitRatePerM,
      after: c.after.unitRatePerM,
      fmt: (v) => `$${trim1(v)}`,
      upIsWorse: true,
      deltaUnit: '$',
    },
    {
      name: t('dash.head.pass'),
      before: c.before.passRatePct,
      after: c.after.passRatePct,
      fmt: (v) => `${trim1(v)}%`,
      upIsWorse: false,
      deltaUnit: '%p',
    },
    {
      name: t('dash.cohort.sigPerSession'),
      before: c.before.signalsPerSession,
      after: c.after.signalsPerSession,
      fmt: (v) => trim1(v),
      upIsWorse: true,
      deltaUnit: '',
    },
    {
      name: t('dash.head.hit'),
      before: c.before.cacheHitPct,
      after: c.after.cacheHitPct,
      fmt: (v) => `${trim1(v)}%`,
      upIsWorse: false,
      deltaUnit: '%p',
    },
  ];
  return (
    <section className="mt-7">
      <div className="mb-2.5 flex items-baseline justify-between">
        <span className="text-[13.5px] font-semibold">
          {t('dash.cohort.compareTitle', label)}
          <small className="ml-2 text-[11.5px] font-medium text-(--wimcc-fg-subtle)">
            {t('dash.cohort.basis')}
          </small>
        </span>
        <span className="font-mono text-[11px] text-(--wimcc-fg-subtle)">
          {t('dash.cohort.beforeAfter', { b: c.before.n, a: c.after.n })}
        </span>
      </div>
      {c.alsoCcChanged && (
        <p className="mb-2 text-[11.5px] text-(--wimcc-fg-subtle)">{t('dash.cohort.ccAlso')}</p>
      )}
      <div className="grid grid-cols-4 gap-3.5">
        {metrics.map((m) => (
          <SlopeCard
            key={m.name}
            m={m}
            lowSample={c.lowSample}
            lowSampleLabel={t('dash.cohort.lowSample')}
            axisLabels={axisLabels}
          />
        ))}
      </div>
    </section>
  );
}
