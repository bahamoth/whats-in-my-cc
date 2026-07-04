/**
 * 코호트 마커(markLine) 라벨 배치 — 순수 함수(테스트 SSOT:
 * __tests__/markerLayout.test.ts). 인접 마커의 라벨 겹침은 세로 스태거 행으로,
 * 플롯 가장자리 클리핑은 정렬 뒤집기로 해소한다. 플롯 폭은 옵션 빌드 시점에
 * 실측할 수 없어 보수적 추정치(px)를 쓴다 — 실제가 더 넓으면 스태거가 약간
 * 과할 뿐 겹침은 생기지 않는다.
 */

export type MarkerPlacement = {
  align: 'left' | 'right';
  /** 0-기반 스태거 행 — 위로 갈수록 커진다. */
  row: number;
  /** ECharts markLine label.distance — 선 끝(위)에서 라벨까지의 px. */
  distance: number;
};

export type MarkerLayout = {
  /** 입력 마커와 같은 순서. */
  placements: MarkerPlacement[];
  /** 최심 스태거 행까지 라벨이 들어가도록 확장한 grid.top(px). */
  gridTop: number;
};

/** 10.5px 모노스페이스 글자폭 근사. */
const CHAR_PX = 6.3;
/** 라벨 좌우 패딩 + 여유. */
const LABEL_PAD_PX = 8;
/** 같은 행에서 라벨 사이 최소 간격. */
const GAP_PX = 8;
export const MARKER_BASE_DISTANCE = 4;
export const MARKER_ROW_PX = 13;
export const MARKER_MAX_ROWS = 3;
export const MARKER_BASE_TOP = 26;
/** 옵션 빌드 시점에 실측 불가한 플롯 폭의 보수적 추정치. */
export const MARKER_PLOT_W_FALLBACK = 720;

export function layoutMarkerLabels(
  markers: Array<{ dayIdx: number; label: string }>,
  nDays: number,
  plotWidthPx: number = MARKER_PLOT_W_FALLBACK,
): MarkerLayout {
  const denom = Math.max(1, nDays - 1);
  const spans = markers.map((m, i) => {
    const anchor = (m.dayIdx / denom) * plotWidthPx;
    const width = m.label.length * CHAR_PX + LABEL_PAD_PX;
    let align: 'left' | 'right' = m.dayIdx / denom > 0.6 ? 'right' : 'left';
    // 가장자리 클리핑 방지 — 넘침이 작은 쪽을 고른다.
    const overRight = Math.max(0, anchor + width - plotWidthPx); // left 정렬 시
    const overLeft = Math.max(0, width - anchor); // right 정렬 시
    if (align === 'left' && overRight > overLeft) align = 'right';
    else if (align === 'right' && overLeft > overRight) align = 'left';
    const start = align === 'left' ? anchor : anchor - width;
    return { i, align, start, end: start + width };
  });

  // 시작 x 순으로 그리디 행 배정 — 같은 행의 직전 라벨과 GAP 미만이면 다음 행.
  const sorted = [...spans].sort((a, b) => a.start - b.start || a.i - b.i);
  const rowEnds: number[] = [];
  const rowOf = new Array<number>(markers.length).fill(0);
  for (const s of sorted) {
    let row = rowEnds.findIndex((end) => s.start >= end + GAP_PX);
    if (row === -1) {
      if (rowEnds.length < MARKER_MAX_ROWS) row = rowEnds.length;
      // 행이 다 찼으면 끝이 가장 이른 행에 겹쳐 배치(결정론적 최선).
      else row = rowEnds.indexOf(Math.min(...rowEnds));
    }
    rowEnds[row] = Math.max(rowEnds[row] ?? -Infinity, s.end);
    rowOf[s.i] = row;
  }

  const maxRow = markers.length ? Math.max(...rowOf) : 0;
  return {
    placements: spans.map((s) => ({
      align: s.align,
      row: rowOf[s.i],
      distance: MARKER_BASE_DISTANCE + rowOf[s.i] * MARKER_ROW_PX,
    })),
    gridTop: MARKER_BASE_TOP + maxRow * MARKER_ROW_PX,
  };
}
