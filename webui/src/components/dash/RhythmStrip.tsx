/** 진행률(0–100%) 축 위 outcome 점 스트립 — 대시보드 GuardRhythm과 세션
 *  분석 패널(검증 리듬)이 공유하는 표현 계층. 색 SSOT: OUTCOME_COLORS. */
import { OUTCOME_COLORS } from './echartsBase';

const DOT: Record<string, string> = {
  passed: OUTCOME_COLORS.passed,
  failed: OUTCOME_COLORS.failed,
  unknown: OUTCOME_COLORS.unknown,
  not_executed: OUTCOME_COLORS.not_executed,
};

export type RhythmStripRun = { pct: number; status: string };

export function RhythmStrip({
  runs,
  onRunClick,
}: {
  runs: RhythmStripRun[];
  /** 점 클릭 — 세션판이 trigger 이벤트 점프에 쓴다. 없으면 정적 렌더. */
  onRunClick?: (index: number) => void;
}) {
  const dotClass = 'absolute top-[5px] h-4 w-2 -translate-x-1 rounded-[2.5px]';
  return (
    <div className="relative h-[26px] min-w-0 flex-1 rounded-md bg-(--wimcc-surface-2)">
      {runs.map((run, i) => {
        const style = {
          left: `${run.pct}%`,
          background: DOT[run.status] ?? OUTCOME_COLORS.unknown,
        };
        const title = `${run.pct}% · ${run.status}`;
        return onRunClick ? (
          <button
            key={i}
            type="button"
            data-dot
            title={title}
            className={`${dotClass} cursor-pointer border-0 p-0`}
            style={style}
            onClick={() => onRunClick(i)}
          />
        ) : (
          <b key={i} data-dot title={title} className={dotClass} style={style} />
        );
      })}
    </div>
  );
}
