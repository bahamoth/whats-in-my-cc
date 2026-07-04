/** 세션 분포 — 사용량 × 신호 밀도의 2차원 발견 뷰(스펙 §1 모듈 6).
 *  점 클릭 시 리플레이. 옵션 파생은 scatterOption(순수, vitest 잠금). */
import { useMemo } from 'react';
import type { SessionSeriesRowDto } from '../../api/types';
import { EChart } from './EChart';
import { buildScatterOption } from './scatterOption';
import { useT } from '../../i18n';
import { InfoTip } from '../replay/insight-strip/InfoTip';

type ScatterPointParams = { data?: { s?: { sid?: string } } };

export function SessionScatter({
  rows,
  nameOf,
  onOpen,
}: {
  rows: SessionSeriesRowDto[];
  nameOf: (sid: string) => string;
  onOpen: (sid: string) => void;
}) {
  const t = useT();
  const { option, points } = useMemo(
    () =>
      buildScatterOption({
        rows,
        nameOf,
        labels: {
          x: t('dash.scatter.x'),
          y: t('dash.scatter.y'),
          unassigned: t('dash.lane.notMeasured'),
          click: t('dash.scatter.click'),
        },
      }),
    [rows, nameOf, t],
  );
  if (points === 0) return null;
  return (
    <section className="mt-7">
      <div className="mb-2.5 flex items-baseline justify-between">
        <span className="text-[13.5px] font-semibold">
          {t('dash.scatter.title')}
          <InfoTip label={t('dash.scatter.title')} text={t('dash.scatter.tip')} />
          <small className="ml-2 text-[11.5px] font-medium text-(--wimcc-fg-subtle)">
            {t('dash.scatter.desc')}
          </small>
        </span>
        <span className="font-mono text-[11px] text-(--wimcc-fg-subtle)">
          {t('dash.scatter.median')}
        </span>
      </div>
      <div className="rounded-[13px] border border-(--wimcc-border) bg-(--wimcc-surface-1) px-4 pt-3 pb-1.5">
        <EChart
          option={option}
          height={400}
          onEvents={{
            click: (p) => {
              const sid = (p as ScatterPointParams).data?.s?.sid;
              if (sid) onOpen(sid);
            },
          }}
        />
      </div>
    </section>
  );
}
