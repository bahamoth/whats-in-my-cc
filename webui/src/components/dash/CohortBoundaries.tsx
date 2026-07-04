/** 코호트 경계 (스펙 §2 3차 개정) — 도구는 차원을 고르지 않는다.
 *  기본 뷰는 초과율 게이트를 통과한 경계(rankCohorts.surfaced, 결정론),
 *  차원 버튼으로 모든 경계를 수동 탐색. 전/후 = 창 내 경계 이전/이후 전체 —
 *  랭킹 통계량과 표시 집계가 같은 정의다. */
import { useMemo, useState } from 'react';
import type { EChartsCoreOption } from 'echarts/core';
import type { SessionSeriesRowDto } from '../../api/types';
import {
  rankCohorts,
  COHORT_DIMS,
  type CohortDim,
  type CohortMetric,
  type RankedBoundary,
} from '../../lib/dashDerive';
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
  key: CohortMetric;
  name: string;
  before: number | null;
  after: number | null;
  fmt: (v: number) => string;
  upIsWorse: boolean;
  deltaUnit: string;
};

function SlopeCard({
  m,
  lowSample,
  lowSampleLabel,
  axisLabels,
}: {
  m: MetricDef;
  lowSample: boolean;
  lowSampleLabel: string;
  axisLabels: [string, string];
}) {
  const has = m.before !== null && m.after !== null;
  const d = has ? Math.round(((m.after as number) - (m.before as number)) * 100) / 100 : null;
  const worse = d !== null && (d > 0 ? m.upIsWorse : d < 0 ? !m.upIsWorse : false);
  return (
    <div className="rounded-[13px] border border-(--wimcc-border) bg-(--wimcc-surface-1) px-4 pt-3.5 pb-1">
      <span className="text-[11.5px] font-semibold text-(--wimcc-fg-muted)">{m.name}</span>
      <div className="my-1 font-mono text-[24px] font-semibold">
        {m.after !== null ? m.fmt(m.after) : '—'}
        {d !== null && !lowSample && (
          <span
            className={`ml-2 rounded-[5px] px-1.5 py-0.5 align-[3px] font-mono text-[11px] ${
              Math.abs(d) < 0.05
                ? 'bg-(--wimcc-surface-2) text-(--wimcc-fg-subtle)'
                : worse
                  ? 'bg-[#f0b429]/10 text-[#f0b429]'
                  : 'bg-[#41c285]/10 text-[#41c285]'
            }`}
          >
            {Math.abs(d) < 0.05 ? '▬ 0.0' : `${d > 0 ? '▲' : '▼'} ${m.deltaUnit === '$' ? '$' + trim1(Math.abs(d)) : trim1(Math.abs(d)) + m.deltaUnit}`}
          </span>
        )}
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

export function CohortBoundaries({ rows }: { rows: SessionSeriesRowDto[] }) {
  const t = useT();
  const [dimFilter, setDimFilter] = useState<'auto' | CohortDim>('auto');
  const [picked, setPicked] = useState<string | null>(null); // `${dim}:${index}`
  const { surfaced, all } = useMemo(() => rankCohorts(rows), [rows]);

  const list = useMemo(() => {
    if (dimFilter === 'auto') return surfaced;
    return all.filter((b) => b.dim === dimFilter).sort((a, b) => b.index - a.index);
  }, [dimFilter, surfaced, all]);

  const sel: RankedBoundary | undefined =
    list.find((b) => `${b.dim}:${b.index}` === picked) ?? list[0];

  if (all.length === 0) return null;

  const dimName = (d: 'auto' | CohortDim) =>
    d === 'auto' ? t('dash.cohort.dim.auto') : t(`dash.cohort.dim.${d}`);
  const labelOf = (b: RankedBoundary) =>
    b.added.length && b.removed.length
      ? `${b.removed.join(' · ')} → ${b.added.join(' · ')}`
      : b.added.length
        ? t('dash.cohort.introduced', b.added.join(' · '))
        : t('dash.cohort.retired', b.removed.join(' · '));
  const axisLabels: [string, string] = [t('dash.cohort.before'), t('dash.cohort.after')];

  const metricName: Record<CohortMetric, string> = {
    unitRate: t('dash.head.rate'),
    passRate: t('dash.head.pass'),
    signals: t('dash.cohort.sigPerSession'),
    cacheHit: t('dash.head.hit'),
  };
  const metrics = (b: RankedBoundary): MetricDef[] => [
    {
      key: 'unitRate',
      name: metricName.unitRate,
      before: b.before.unitRatePerM,
      after: b.after.unitRatePerM,
      fmt: (v) => `$${trim1(v)}`,
      upIsWorse: true,
      deltaUnit: '$',
    },
    {
      key: 'passRate',
      name: metricName.passRate,
      before: b.before.passRatePct,
      after: b.after.passRatePct,
      fmt: (v) => `${trim1(v)}%`,
      upIsWorse: false,
      deltaUnit: '%p',
    },
    {
      key: 'signals',
      name: metricName.signals,
      before: b.before.signalsPerSession,
      after: b.after.signalsPerSession,
      fmt: (v) => trim1(v),
      upIsWorse: true,
      deltaUnit: '',
    },
    {
      key: 'cacheHit',
      name: metricName.cacheHit,
      before: b.before.cacheHitPct,
      after: b.after.cacheHitPct,
      fmt: (v) => `${trim1(v)}%`,
      upIsWorse: false,
      deltaUnit: '%p',
    },
  ];

  return (
    <section className="mt-7">
      <div className="mb-2.5 flex flex-wrap items-baseline justify-between gap-2">
        <span className="text-[13.5px] font-semibold">
          {t('dash.cohort.secTitle')}
          <small className="ml-2 text-[11.5px] font-medium text-(--wimcc-fg-subtle)">
            {t('dash.cohort.prefixNote')}
          </small>
        </span>
        <div className="flex gap-1">
          {(['auto', ...COHORT_DIMS] as const).map((d) => (
            <button
              key={d}
              type="button"
              onClick={() => {
                setDimFilter(d);
                setPicked(null);
              }}
              className={`rounded-md px-2 py-0.5 font-mono text-[10.5px] transition-colors ${
                dimFilter === d
                  ? 'bg-(--wimcc-surface-3) text-(--wimcc-fg)'
                  : 'text-(--wimcc-fg-subtle) hover:text-(--wimcc-fg-muted)'
              }`}
            >
              {dimName(d)}
            </button>
          ))}
        </div>
      </div>

      {list.length === 0 && (
        <p className="mb-2 text-[12px] text-(--wimcc-fg-subtle)">{t('dash.cohort.noneAuto')}</p>
      )}
      {list.length > 0 && (
        <div className="mb-3 flex flex-wrap gap-1.5">
          {list.map((b) => {
            const active = sel === b;
            return (
              <button
                key={`${b.dim}:${b.index}`}
                type="button"
                onClick={() => setPicked(`${b.dim}:${b.index}`)}
                className={`rounded-lg border px-2.5 py-1.5 text-left font-mono text-[10.5px] transition-colors ${
                  active
                    ? 'border-(--wimcc-border-strong) bg-(--wimcc-surface-2)'
                    : 'border-(--wimcc-border) bg-(--wimcc-surface-1) hover:border-(--wimcc-border-strong)'
                }`}
              >
                <span className="text-(--wimcc-fg-subtle)">{b.date}</span>{' '}
                <span className="font-semibold">{labelOf(b)}</span>
                {dimFilter === 'auto' && (
                  <span className="ml-1.5 text-(--wimcc-fg-subtle)">[{dimName(b.dim)}]</span>
                )}
                <span className="ml-1.5 text-[#b07dff]">
                  {b.exceed !== null && b.bestMetric
                    ? `Δ${metricName[b.bestMetric]} · ${t('dash.cohort.exceed', Math.max(1, Math.round(b.exceed * 100)))}`
                    : t('dash.cohort.lowSample')}
                </span>
              </button>
            );
          })}
        </div>
      )}

      {sel && (
        <>
          <div className="mb-2 flex items-baseline justify-between">
            <span className="text-[12.5px] font-medium text-(--wimcc-fg-muted)">
              {t('dash.cohort.compareTitle', labelOf(sel))}
              {sel.alsoChanged.length > 0 && (
                <span className="ml-2 text-[11px] text-(--wimcc-fg-subtle)">
                  {t('dash.cohort.alsoDims', sel.alsoChanged.map((d) => dimName(d)).join(' · '))}
                </span>
              )}
            </span>
            <span className="font-mono text-[11px] text-(--wimcc-fg-subtle)">
              {t('dash.cohort.beforeAfter', { b: sel.before.n, a: sel.after.n })}
            </span>
          </div>
          <div className="grid grid-cols-4 gap-3.5">
            {metrics(sel).map((m) => (
              <SlopeCard
                key={m.key}
                m={m}
                lowSample={sel.exceed === null}
                lowSampleLabel={t('dash.cohort.lowSample')}
                axisLabels={axisLabels}
              />
            ))}
          </div>
        </>
      )}
    </section>
  );
}
