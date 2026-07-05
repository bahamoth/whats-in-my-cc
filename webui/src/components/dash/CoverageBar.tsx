/** 커버/미커버 hunk 2분할 바 — 대시보드 ChangeCoverage 행과 세션 분석
 *  패널이 공유. 커버 초록 #41c285 · 미커버 앰버 #f0b429(승인 목업 값). */
export function CoverageBar({ covered, total }: { covered: number; total: number }) {
  const pct = total > 0 ? Math.round((covered / total) * 100) : 0;
  return (
    <div
      data-coverage-bar
      className="flex h-[18px] min-w-0 flex-1 overflow-hidden rounded-[5px] bg-(--wimcc-surface-2)"
    >
      <i style={{ width: `${pct}%`, background: '#41c285', opacity: 0.9 }} />
      <i style={{ width: `${100 - pct}%`, background: '#f0b429', opacity: 0.75 }} />
    </div>
  );
}
