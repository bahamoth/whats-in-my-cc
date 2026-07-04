/** 검증 결과 — 일별 스택(통과/실패/판정불가). 개요 탭의 첫 차트 모듈 —
 *  가장 인사이트 있는 데이터를 헤드라인 바로 아래 둔다(스펙 §1 모듈 2). */
import { useMemo } from 'react';
import type { Daily } from '../../lib/dashDerive';
import { EChart } from './EChart';
import { buildVerOption, type CohortMarker, type DayDetail } from './dailyOptions';
import { OUTCOME_COLORS } from './echartsBase';
import { useT } from '../../i18n';
import { InfoTip } from '../replay/insight-strip/InfoTip';

export function DailyVerification({
  daily,
  markers,
  details,
  zeroGuards,
  guards,
  passed,
}: {
  daily: Daily;
  markers: CohortMarker[];
  details: DayDetail[][];
  zeroGuards: number;
  guards: number;
  passed: number;
}) {
  const t = useT();
  const option = useMemo(
    () =>
      buildVerOption({
        daily,
        markers,
        details,
        labels: {
          passed: t('dash.outcome.passed'),
          failed: t('dash.outcome.failed'),
          unknown: t('dash.outcome.unknown'),
          noGuards: t('dash.tt.noGuards'),
        },
      }),
    [daily, markers, details, t],
  );
  return (
    <section className="mt-7">
      <div className="mb-2.5 flex items-baseline justify-between">
        <span className="text-[13.5px] font-semibold">
          {t('dash.daily.ver.title')}
          <InfoTip label={t('dash.daily.ver.title')} text={t('dash.daily.ver.tip')} />
          <small className="ml-2 text-[11.5px] font-medium text-(--wimcc-fg-subtle)">
            <b style={{ color: OUTCOME_COLORS.passed }}>{t('dash.outcome.passed')}</b>
            {' · '}
            <b style={{ color: OUTCOME_COLORS.failed }}>{t('dash.outcome.failed')}</b>
            {' · '}
            <b style={{ color: '#6a7180' }}>{t('dash.outcome.unknown')}</b>
            {' — '}
            {t('dash.daily.ver.zeroGuards', zeroGuards)}
          </small>
        </span>
        <span className="font-mono text-[11px] text-(--wimcc-fg-subtle)">
          {t('dash.daily.ver.badge', { n: guards, m: passed })}
        </span>
      </div>
      <div className="rounded-[13px] border border-(--wimcc-border) bg-(--wimcc-surface-1) px-4 pt-3 pb-1.5">
        <EChart option={option} height={280} group="dash-days" />
      </div>
    </section>
  );
}
