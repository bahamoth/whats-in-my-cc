/** 기간 컨트롤(2026-07-04 피드백) — 프리셋(30/90일·전체) + Calendar 범위 선택을
 *  하나의 Popover로. 차트 내부 레인지 컨트롤(dataZoom)은 스크롤과 충돌해
 *  제거했고, 기간 변경은 이 컨트롤이 전담한다. */
import { useState } from 'react';
import type { DateRange } from 'react-day-picker';
import { Button } from '@/components/ui/button';
import { Calendar } from '@/components/ui/calendar';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { useT } from '../../i18n';

export type WindowSel =
  | { kind: '30d' | '90d' | 'all' }
  | { kind: 'custom'; from: string; to: string }; // YYYY-MM-DD

const iso = (d: Date) =>
  `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;

export function DateRangeControl({
  sel,
  onChange,
}: {
  sel: WindowSel;
  onChange: (next: WindowSel) => void;
}) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const [draft, setDraft] = useState<DateRange | undefined>(undefined);

  const label =
    sel.kind === 'custom'
      ? `${sel.from.slice(5)} → ${sel.to.slice(5)}`
      : sel.kind === '30d'
        ? t('dash.range.last30')
        : sel.kind === '90d'
          ? t('dash.range.last90')
          : t('dash.range.all');

  const preset = (kind: '30d' | '90d' | 'all') => {
    onChange({ kind });
    setOpen(false);
  };

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button variant="outline" size="sm" className="gap-2 font-mono text-xs">
          <span aria-hidden>▦</span>
          {label}
        </Button>
      </PopoverTrigger>
      <PopoverContent align="end" className="w-auto p-0">
        <div className="flex">
          <div className="flex flex-col gap-1 border-r border-(--wimcc-border) p-2">
            {(['30d', '90d', 'all'] as const).map((k) => (
              <Button
                key={k}
                variant={sel.kind === k ? 'secondary' : 'ghost'}
                size="sm"
                className="justify-start text-xs"
                onClick={() => preset(k)}
              >
                {k === '30d' ? t('dash.range.last30') : k === '90d' ? t('dash.range.last90') : t('dash.range.all')}
              </Button>
            ))}
            <p className="mt-1 max-w-[130px] px-2 text-[10.5px] leading-snug text-(--wimcc-fg-subtle)">
              {t('dash.range.hint')}
            </p>
          </div>
          <Calendar
            mode="range"
            numberOfMonths={2}
            selected={draft}
            defaultMonth={
              sel.kind === 'custom'
                ? new Date(sel.from)
                : new Date(new Date().setMonth(new Date().getMonth() - 1))
            }
            onSelect={(r) => {
              setDraft(r);
              if (r?.from && r?.to && r.from.getTime() !== r.to.getTime()) {
                onChange({ kind: 'custom', from: iso(r.from), to: iso(r.to) });
                setOpen(false);
                setDraft(undefined);
              }
            }}
          />
        </div>
      </PopoverContent>
    </Popover>
  );
}
