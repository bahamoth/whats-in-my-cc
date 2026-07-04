/** 변경 커버리지 — 검증 통과가 거친 diff hunk 비율(스펙 §3). 미커버가
 *  앰버로 튄다. 좌표계 없는 바 리스트라 DOM으로 그린다. */
import type { VerificationSummaryDto } from '../../api/types';
import { useT } from '../../i18n';
import { InfoTip } from '../replay/insight-strip/InfoTip';

export function ChangeCoverage({
  coverage,
  nameOf,
}: {
  coverage: VerificationSummaryDto['coverage'];
  nameOf: (sid: string) => string;
}) {
  const t = useT();
  if (coverage.total === 0) return null;
  const overallPct = Math.round((coverage.covered / coverage.total) * 100);
  return (
    <section className="mt-7">
      <div className="mb-2.5 flex items-baseline justify-between">
        <span className="text-[13.5px] font-semibold">
          {t('dash.ver.cov.title')}
          <InfoTip label={t('dash.ver.cov.title')} text={t('dash.ver.cov.tip')} />
          <small className="ml-2 text-[11.5px] font-medium text-(--wimcc-fg-subtle)">
            {t('dash.ver.cov.desc')}
          </small>
        </span>
        <span className="font-mono text-[11px] text-(--wimcc-fg-subtle)">
          {t('dash.ver.cov.overall', {
            pct: overallPct,
            n: coverage.total - coverage.covered,
          })}
        </span>
      </div>
      <div className="rounded-[13px] border border-(--wimcc-border) bg-(--wimcc-surface-1) px-4 pt-4 pb-2.5">
        {coverage.by_session.map((s) => {
          const pct = s.total > 0 ? Math.round((s.covered / s.total) * 100) : 0;
          return (
            <div key={s.session_id} className="mb-2.5 flex items-center last:mb-1">
              <div
                className="w-[210px] flex-none truncate pr-3.5 font-mono text-[11.5px]"
                title={nameOf(s.session_id)}
              >
                {nameOf(s.session_id)}
              </div>
              <div className="flex h-[18px] min-w-0 flex-1 overflow-hidden rounded-[5px] bg-(--wimcc-surface-2)">
                <i style={{ width: `${pct}%`, background: '#41c285', opacity: 0.9 }} />
                <i style={{ width: `${100 - pct}%`, background: '#f0b429', opacity: 0.75 }} />
              </div>
              <div className="w-[150px] flex-none text-right font-mono text-[11px] text-(--wimcc-fg-muted)">
                {pct}% · <b className="text-[#f0b429]">{s.total - s.covered}</b>
              </div>
            </div>
          );
        })}
      </div>
    </section>
  );
}
