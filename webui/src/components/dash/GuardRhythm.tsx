/** 가드 실행 리듬 — 세션 진행률(시간 기준) 위 outcome 점 스트립(스펙 §3).
 *  패턴(빨강→초록 교차 / 끝몰림 / 마스킹 회색)이 스스로 말한다 — 판정 없음. */
import type { VerificationSummaryDto } from '../../api/types';
import { OUTCOME_COLORS } from './echartsBase';
import { useT } from '../../i18n';

const DOT: Record<string, string> = {
  passed: OUTCOME_COLORS.passed,
  failed: OUTCOME_COLORS.failed,
  unknown: OUTCOME_COLORS.unknown,
  not_executed: OUTCOME_COLORS.not_executed,
};

export function GuardRhythm({
  rhythm,
  nameOf,
}: {
  rhythm: VerificationSummaryDto['rhythm'];
  nameOf: (sid: string) => string;
}) {
  const t = useT();
  if (rhythm.length === 0) return null;
  return (
    <section className="mt-7">
      <div className="mb-2.5 flex items-baseline justify-between">
        <span className="text-[13.5px] font-semibold">
          {t('dash.ver.rhythm.title')}
          <small className="ml-2 text-[11.5px] font-medium text-(--wimcc-fg-subtle)">
            {t('dash.ver.rhythm.desc')}
          </small>
        </span>
        <span className="font-mono text-[11px] text-(--wimcc-fg-subtle)">
          {t('dash.ver.rhythm.axis')}
        </span>
      </div>
      <div className="rounded-[13px] border border-(--wimcc-border) bg-(--wimcc-surface-1) px-4 pt-4 pb-3">
        {rhythm.map((r) => (
          <div key={r.session_id} className="mb-3.5 flex items-center last:mb-1">
            <div className="w-[210px] flex-none pr-3.5">
              <div className="truncate font-mono text-[11.5px] font-semibold" title={nameOf(r.session_id)}>
                {nameOf(r.session_id)}
              </div>
              <div className="mt-0.5 font-mono text-[10.5px] text-(--wimcc-fg-subtle)">
                {t('dash.ver.rhythm.meta', { g: r.guards, p: r.passed })}
              </div>
            </div>
            <div className="relative h-[26px] min-w-0 flex-1 rounded-md bg-(--wimcc-surface-2)">
              {r.runs.map((run, i) => (
                <b
                  key={i}
                  data-dot
                  title={`${run.pct}% · ${run.status}`}
                  className="absolute top-[5px] h-4 w-2 -translate-x-1 rounded-[2.5px]"
                  style={{ left: `${run.pct}%`, background: DOT[run.status] ?? OUTCOME_COLORS.unknown }}
                />
              ))}
            </div>
          </div>
        ))}
        <div className="mt-1.5 ml-[210px] flex justify-between font-mono text-[9.5px] text-(--wimcc-fg-subtle)">
          <span>0%</span>
          <span>25%</span>
          <span>50%</span>
          <span>75%</span>
          <span>100%</span>
        </div>
      </div>
    </section>
  );
}
