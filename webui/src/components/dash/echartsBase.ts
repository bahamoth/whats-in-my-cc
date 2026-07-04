/**
 * 대시보드 ECharts 공통 상수 — 승인 목업(docs/mockups/dash-full-mockup.html)의
 * 툴팁/축 스타일과 신호 히트 램프를 그대로 상수화한 SSOT.
 */
export const TOOLTIP = {
  backgroundColor: '#1b202a',
  borderColor: '#2a3040',
  borderWidth: 1,
  padding: [9, 12],
  textStyle: { color: '#e6e8ee', fontSize: 12 },
  extraCssText: 'border-radius:10px;box-shadow:0 14px 40px -18px rgba(0,0,0,.9)',
} as const;

export const AXIS_LABEL = {
  color: '#6a7180',
  fontFamily: 'ui-monospace,Menlo,monospace',
  fontSize: 10.5,
} as const;

export const SPLIT_LINE = { lineStyle: { color: '#171b24' } } as const;

export const OUTCOME_COLORS = {
  passed: '#41c285',
  failed: '#ef4747',
  unknown: '#4a5162',
  not_executed: '#3d4351',
} as const;

/* 신호 히트 램프: 초록(0) → 앰버 → 적(1). 일별 비용 막대의 색 = 그날 신호 수. */
const RAMP: Array<[number, string]> = [
  [0, '#41c285'],
  [0.35, '#c9c04a'],
  [0.7, '#f0a03c'],
  [1, '#ef6047'],
];

export function rampColor(t: number): string {
  const cl = Math.max(0, Math.min(1, t));
  const hx = (c: string) => [1, 3, 5].map((i) => parseInt(c.slice(i, i + 2), 16));
  for (let k = 1; k < RAMP.length; k++) {
    if (cl <= RAMP[k][0]) {
      const [t0, c0] = RAMP[k - 1];
      const [t1, c1] = RAMP[k];
      const u = t1 === t0 ? 0 : (cl - t0) / (t1 - t0);
      const a = hx(c0);
      const b = hx(c1);
      return (
        '#' +
        a
          .map((v, j) =>
            Math.round(v + (b[j] - v) * u)
              .toString(16)
              .padStart(2, '0'),
          )
          .join('')
      );
    }
  }
  return RAMP[RAMP.length - 1][1];
}
