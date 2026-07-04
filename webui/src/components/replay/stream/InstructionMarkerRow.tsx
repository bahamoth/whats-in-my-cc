/** 지시문 변경 관측 마커 행 (B-12) — 보라 점선(경계 문법)으로 서사를 가른다.
 *  wimcc 파생 항목이므로 Derived 배지를 달고, 클릭하면 내용 diff를 펼친다. */
import { useState } from 'react';
import type { InstructionMarkerItem } from './streamModel';
import { InstructionDiff } from '../../dash/InstructionDiff';
import { clockLabel } from '../../../lib/format';
import { useT } from '../../../i18n';

export function InstructionMarkerRow({ item }: { item: InstructionMarkerItem }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  return (
    <div className="my-1.5">
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        className="flex w-full items-center gap-2 px-1 py-0.5 text-left"
        title={item.observedAt}
      >
        <span className="h-px flex-1 border-t border-dashed border-[#b07dff]/60" aria-hidden />
        <span className="shrink-0 font-mono text-[10.5px] text-[#b07dff]">
          {t('instr.marker.label', {
            source: item.source,
            // 가터와 동일한 로컬 시계 — ISO 슬라이스(UTC)는 가터와 어긋난다.
            time: clockLabel(Date.parse(item.observedAt)),
          })}
        </span>
        <span className="shrink-0 rounded-[4px] border border-(--wimcc-border) px-1 font-mono text-[9px] text-(--wimcc-fg-subtle)">
          {t('detail.provenance.derived')}
        </span>
        <span className="shrink-0 font-mono text-[10px] text-(--wimcc-fg-subtle)">
          {open ? t('instr.marker.hide') : t('instr.marker.show')}
        </span>
        <span className="h-px flex-1 border-t border-dashed border-[#b07dff]/60" aria-hidden />
      </button>
      {open && (
        <div className="px-4">
          <InstructionDiff
            source={item.source}
            beforeHash={item.beforeHash}
            afterHash={item.afterHash}
          />
        </div>
      )}
    </div>
  );
}
