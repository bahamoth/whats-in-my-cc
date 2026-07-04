/** 일별 비용 · 신호 — 막대 높이=비용, 막대 색=그날 신호 수(연속 램프).
 *  코호트 전환 markLine 관통, dataZoom으로 구간 확대(스펙 §1 모듈 3). */
import { useMemo } from 'react';
import type { Daily } from '../../lib/dashDerive';
import { EChart } from './EChart';
import { buildCostOption, type CohortMarker, type DayDetail } from './dailyOptions';
import { useT } from '../../i18n';

export function DailyCostSignals({
  daily,
  markers,
  details,
}: {
  daily: Daily;
  markers: CohortMarker[];
  details: DayDetail[][];
}) {
  const t = useT();
  const sigMax = Math.max(...daily.signals, 1);
  const total = daily.cost.reduce((a, v) => a + v, 0);
  const option = useMemo(
    () =>
      buildCostOption({
        daily,
        markers,
        details,
        labels: { signals: t('dash.tt.signals'), noSessions: t('dash.tt.noSessions') },
      }),
    [daily, markers, details, t],
  );
  return (
    <section className="mt-7">
      <div className="mb-2.5 flex items-baseline justify-between">
        <span className="text-[13.5px] font-semibold">
          {t('dash.daily.cost.title')}
          <small className="ml-2 text-[11.5px] font-medium text-(--wimcc-fg-subtle)">
            {t('dash.daily.cost.desc')}
            <span
              className="mx-1.5 inline-block h-2 w-[74px] rounded-[4px] align-[-1px]"
              style={{
                background: 'linear-gradient(90deg,#41c285,#c9c04a 35%,#f0a03c 70%,#ef6047)',
              }}
            />
            <span className="font-mono">0 → {sigMax}</span>
          </small>
        </span>
        <span className="font-mono text-[11px] text-(--wimcc-fg-subtle)">
          {t('dash.cost.total', total.toFixed(2))}
        </span>
      </div>
      <div className="rounded-[13px] border border-(--wimcc-border) bg-(--wimcc-surface-1) px-4 pt-3 pb-1.5">
        <EChart option={option} height={330} group="dash-days" />
      </div>
    </section>
  );
}
