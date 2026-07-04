/** 검증 탭(스펙 §3) — 문자 헤드라인 5칸 → kind 구성 · 가드 행방(Sankey) →
 *  실행 리듬 → 변경 커버리지. 데이터는 /v1/verification/summary 집계. */
import { useMemo } from 'react';
import type { VerificationSummaryDto } from '../../api/types';
import { EChart } from './EChart';
import { buildKindOption, buildSankeyOption } from './verificationOptions';
import { GuardRhythm } from './GuardRhythm';
import { ChangeCoverage } from './ChangeCoverage';
import { OUTCOME_COLORS } from './echartsBase';
import { useT } from '../../i18n';

function Stat({
  label,
  value,
  unit,
  chip,
  chipTone,
  fnote,
}: {
  label: string;
  value: string;
  unit?: string;
  chip?: string;
  chipTone?: 'warn' | 'bad' | 'good';
  fnote?: string;
}) {
  const tone =
    chipTone === 'bad'
      ? 'bg-[#ef4747]/10 text-[#ef4747]'
      : chipTone === 'good'
        ? 'bg-[#41c285]/10 text-[#41c285]'
        : 'bg-[#f0b429]/10 text-[#f0b429]';
  return (
    <div className="border-r border-(--wimcc-border) px-5 pt-4 pb-3.5 last:border-r-0">
      <div className="text-[10.5px] font-semibold tracking-[.09em] uppercase text-(--wimcc-fg-subtle)">
        {label}
      </div>
      <div className="my-1.5 font-mono text-[29px] leading-none font-semibold tracking-tight">
        {value}
        {unit && <span className="ml-0.5 text-[15px] font-medium text-(--wimcc-fg-muted)">{unit}</span>}
      </div>
      <div className="flex items-center gap-2">
        {chip && (
          <span className={`rounded-[5px] px-1.5 py-0.5 font-mono text-[11.5px] whitespace-nowrap ${tone}`}>
            {chip}
          </span>
        )}
        {fnote && <span className="truncate text-[11px] text-(--wimcc-fg-subtle)">{fnote}</span>}
      </div>
    </div>
  );
}

export function VerificationTab({
  sum,
  nameOf,
}: {
  sum: VerificationSummaryDto;
  nameOf: (sid: string) => string;
}) {
  const t = useT();
  const kindOption = useMemo(
    () =>
      buildKindOption(sum, {
        passed: t('dash.outcome.passed'),
        failed: t('dash.outcome.failed'),
        unknown: t('dash.outcome.unknown'),
        notExec: t('dash.ver.notExec'),
      }),
    [sum, t],
  );
  const sankeyOption = useMemo(
    () =>
      buildSankeyOption(sum, {
        guards: t('dash.ver.node.guards'),
        measured: t('dash.ver.node.measured'),
        unknown: t('dash.outcome.unknown'),
        notExec: t('dash.ver.notExec'),
        passed: t('dash.outcome.passed'),
        failed: t('dash.outcome.failed'),
        recovered: t('dash.ver.node.recovered'),
        abandoned: t('dash.ver.node.abandoned'),
        piped: t('dash.ver.node.piped'),
        other: t('dash.ver.node.other'),
      }),
    [sum, t],
  );
  const measuredPct = sum.total > 0 ? Math.round((sum.measured / sum.total) * 100) : 0;
  const passPct = sum.measured > 0 ? Math.round((sum.passed / sum.measured) * 100) : null;
  const covPct =
    sum.coverage.total > 0 ? Math.round((sum.coverage.covered / sum.coverage.total) * 100) : null;
  const kindsNote = sum.by_kind.map((k) => `${k.kind} ${k.passed + k.failed + k.unknown + k.not_executed}`).join(' · ');

  return (
    <div>
      <div className="grid grid-cols-5 border-y border-(--wimcc-border)">
        <Stat label={t('dash.ver.head.guards')} value={String(sum.total)} fnote={kindsNote} />
        <Stat
          label={t('dash.ver.head.measured')}
          value={String(sum.measured)}
          unit={` · ${measuredPct}%`}
          chip={t('dash.ver.head.unknownChip', sum.unknown)}
          chipTone="warn"
          fnote={t('dash.ver.head.unknownSplit', { p: sum.unknown_piped, o: sum.unknown_other })}
        />
        <Stat
          label={t('dash.ver.head.passed')}
          value={passPct !== null ? String(sum.passed) : '—'}
          unit={passPct !== null ? ` · ${passPct}%` : undefined}
        />
        <Stat
          label={t('dash.ver.head.abandoned')}
          value={String(sum.failures.abandoned)}
          chip={t('dash.ver.head.abandonedOf', sum.failed)}
          chipTone="bad"
          fnote={t('dash.ver.head.abandonedNote')}
        />
        <Stat
          label={t('dash.ver.head.coverage')}
          value={covPct !== null ? String(covPct) : '—'}
          unit={covPct !== null ? '%' : undefined}
          chip={
            covPct !== null
              ? t('dash.ver.cov.uncovered', sum.coverage.total - sum.coverage.covered)
              : undefined
          }
          chipTone="warn"
          fnote={t('dash.ver.head.coverageNote', sum.coverage.total)}
        />
      </div>

      <div className="mt-7 grid grid-cols-2 gap-4">
        <div>
          <div className="mb-2.5 flex items-baseline justify-between">
            <span className="text-[13.5px] font-semibold">
              {t('dash.ver.kind.title')}
              <small className="ml-2 text-[11.5px] font-medium text-(--wimcc-fg-subtle)">
                {t('dash.ver.kind.desc')}
              </small>
            </span>
          </div>
          <div className="rounded-[13px] border border-(--wimcc-border) bg-(--wimcc-surface-1) px-4 pt-3 pb-1.5">
            <EChart option={kindOption} height={250} />
          </div>
        </div>
        <div>
          <div className="mb-2.5 flex items-baseline justify-between">
            <span className="text-[13.5px] font-semibold">
              {t('dash.ver.flow.title', sum.total)}
              <small className="ml-2 text-[11.5px] font-medium text-(--wimcc-fg-subtle)">
                {t('dash.ver.flow.desc')}
              </small>
            </span>
            <span
              className="font-mono text-[11px]"
              style={{ color: sum.failures.abandoned > 0 ? '#ff6b6b' : OUTCOME_COLORS.passed }}
            >
              {t('dash.ver.node.abandoned')} {sum.failures.abandoned}
            </span>
          </div>
          <div className="rounded-[13px] border border-(--wimcc-border) bg-(--wimcc-surface-1) px-4 pt-3 pb-1.5">
            <EChart option={sankeyOption} height={250} />
          </div>
        </div>
      </div>

      <GuardRhythm rhythm={sum.rhythm} nameOf={nameOf} />
      <ChangeCoverage coverage={sum.coverage} nameOf={nameOf} />
    </div>
  );
}
