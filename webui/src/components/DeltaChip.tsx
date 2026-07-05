/**
 * PR-3 §3a — 대시보드 HeadlineStats에서 추출한 공용 delta 칩.
 * ▲/▼/▬ + betterUp 방향색(좋음 #41c285 / 나쁨 #f0b429)으로 방향만 전달한다
 * (측정/판별 분리 — 판정 단어 없음). 마크업·클래스는 추출 전과 동일.
 */
export const trim1 = (v: number) => String(Math.round(v * 10) / 10);

export function DeltaChip({
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
