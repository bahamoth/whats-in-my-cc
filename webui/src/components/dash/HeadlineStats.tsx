/**
 * 문자 헤드라인 밴드 — 결정론 지표 5칸을 큰 타이포로. delta 칩은 이전 동일
 * 창 대비 방향을 색으로 전달한다(측정/판별 분리 — 판정 단어는 없다).
 * 값이 null이면 '—'(미측정 ≠ 0 원칙).
 */
import type { Headline, HeadlineDelta } from '../../lib/dashDerive';
import { useT } from '../../i18n';
import { InfoTip } from '../replay/insight-strip/InfoTip';

const trim1 = (v: number) => String(Math.round(v * 10) / 10);
const money = (v: number) =>
  '$' + v.toLocaleString('en-US', { maximumFractionDigits: v >= 100 ? 0 : 2 });

function DeltaChip({
  v,
  unit,
  betterUp,
  noCompare,
}: {
  v: number | null;
  unit: string;
  betterUp: boolean;
  noCompare: string;
}) {
  if (v === null)
    return <span className="text-[11px] text-(--wimcc-fg-subtle)">{noCompare}</span>;
  const flat = Math.abs(v) < 0.05;
  const good = v > 0 ? betterUp : !betterUp;
  const cls = flat
    ? 'text-(--wimcc-fg-subtle) bg-(--wimcc-surface-2)'
    : good
      ? 'text-[#41c285] bg-[#41c285]/10'
      : 'text-[#f0b429] bg-[#f0b429]/10';
  const arrow = flat ? '▬' : v > 0 ? '▲' : '▼';
  const num = flat ? '0.0' : `${trim1(Math.abs(v))}${unit}`;
  return (
    <span className={`rounded-[5px] px-1.5 py-0.5 font-mono text-[11.5px] whitespace-nowrap ${cls}`}>
      {arrow} {num}
    </span>
  );
}

export function HeadlineStats({ h, d }: { h: Headline; d: HeadlineDelta | null }) {
  const t = useT();
  const noCompare = t('dash.head.noCompare');
  const cells: Array<{
    label: string;
    tip: string;
    value: string;
    unit?: string;
    delta: number | null;
    deltaUnit: string;
    betterUp: boolean;
    fnote: string;
  }> = [
    {
      label: t('dash.head.pass'),
      tip: t('dash.head.pass.tip'),
      value: h.passRatePct !== null ? trim1(h.passRatePct) : '—',
      unit: h.passRatePct !== null ? '%' : undefined,
      delta: d?.passRatePp ?? null,
      deltaUnit: '%p',
      betterUp: true,
      fnote: t('dash.head.guards', h.guards),
    },
    {
      label: t('dash.head.cost'),
      tip: t('dash.head.cost.tip'),
      value: money(h.cost),
      delta: d?.cost ?? null,
      deltaUnit: '$',
      betterUp: false,
      fnote: t('dash.head.costBasis'),
    },
    {
      label: t('dash.head.rate'),
      tip: t('dash.head.rate.tip'),
      value: h.unitRatePerM !== null ? `$${trim1(h.unitRatePerM)}` : '—',
      unit: h.unitRatePerM !== null ? '/1M' : undefined,
      delta: d?.unitRate ?? null,
      deltaUnit: '$',
      betterUp: false,
      fnote: t('dash.head.ratePer'),
    },
    {
      label: t('dash.head.hit'),
      tip: t('dash.head.hit.tip'),
      value: h.cacheHitPct !== null ? trim1(h.cacheHitPct) : '—',
      unit: h.cacheHitPct !== null ? '%' : undefined,
      delta: d?.cacheHitPp ?? null,
      deltaUnit: '%p',
      betterUp: true,
      fnote: t('dash.head.hitBasis'),
    },
    {
      label: t('dash.head.toolfail'),
      tip: t('dash.head.toolfail.tip'),
      value: h.toolFailPct !== null ? trim1(h.toolFailPct) : '—',
      unit: h.toolFailPct !== null ? '%' : undefined,
      delta: d?.toolFailPp ?? null,
      deltaUnit: '%p',
      betterUp: false,
      fnote: t('dash.head.toolfailOf', {
        fails: h.toolFails,
        calls: h.toolCalls.toLocaleString(),
      }),
    },
  ];
  return (
    <div className="grid grid-cols-5 border-y border-(--wimcc-border)">
      {cells.map((c) => (
        <div key={c.label} className="border-r border-(--wimcc-border) px-5 pt-4 pb-3.5 last:border-r-0">
          <div className="flex items-center gap-1 text-[10.5px] font-semibold tracking-[.09em] uppercase text-(--wimcc-fg-subtle)">
            {c.label}
            <InfoTip label={c.label} text={c.tip} />
          </div>
          <div className="my-1.5 font-mono text-[29px] leading-none font-semibold tracking-tight">
            {c.value}
            {c.unit && <span className="ml-0.5 text-[15px] font-medium text-(--wimcc-fg-muted)">{c.unit}</span>}
          </div>
          <div className="flex items-center gap-2">
            <DeltaChip v={c.delta} unit={c.deltaUnit} betterUp={c.betterUp} noCompare={noCompare} />
            <span className="truncate text-[11px] text-(--wimcc-fg-subtle)">{c.fnote}</span>
          </div>
        </div>
      ))}
    </div>
  );
}
