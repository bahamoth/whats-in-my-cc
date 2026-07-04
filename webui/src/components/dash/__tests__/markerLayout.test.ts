/**
 * 코호트 마커 라벨 배치 — 순수 함수. 인접 마커 라벨의 겹침(세로 스태거 행)과
 * 플롯 가장자리 클리핑(정렬 뒤집기)을 결정론적으로 잠근다.
 */
import { describe, expect, it } from 'vitest';
import {
  layoutMarkerLabels,
  MARKER_BASE_DISTANCE,
  MARKER_BASE_TOP,
  MARKER_ROW_PX,
} from '../markerLayout';

const m = (dayIdx: number, label: string) => ({ dayIdx, label });

describe('layoutMarkerLabels', () => {
  it('멀리 떨어진 마커는 전부 0행 — distance·gridTop 기본값', () => {
    const { placements, gridTop } = layoutMarkerLabels([m(2, 'AA'), m(25, 'BB')], 30, 720);
    expect(placements.map((p) => p.row)).toEqual([0, 0]);
    expect(placements.map((p) => p.distance)).toEqual([
      MARKER_BASE_DISTANCE,
      MARKER_BASE_DISTANCE,
    ]);
    expect(gridTop).toBe(MARKER_BASE_TOP);
  });

  it('인접 마커는 행을 나눠 세로 스태거 — distance 증가 + gridTop 확장', () => {
    const { placements, gridTop } = layoutMarkerLabels(
      [m(10, 'Fable 5 → Opus 4.8'), m(11, 'Claude Code 2.1 → 2.2')],
      30,
      720,
    );
    expect(placements[0].row).toBe(0);
    expect(placements[1].row).toBe(1);
    expect(placements[1].distance).toBe(MARKER_BASE_DISTANCE + MARKER_ROW_PX);
    expect(gridTop).toBe(MARKER_BASE_TOP + MARKER_ROW_PX);
  });

  it('우측 60% 초과는 right 정렬, 좌측은 left 정렬(기존 규칙 흡수)', () => {
    const { placements } = layoutMarkerLabels([m(2, 'AA'), m(28, 'BB')], 30, 720);
    expect(placements[0].align).toBe('left');
    expect(placements[1].align).toBe('right');
  });

  it('left 정렬 라벨이 플롯 우측 밖으로 나가면 right로 뒤집는다', () => {
    // frac 0.5 ≤ 0.6 → 초기 left지만, 라벨 폭이 남은 플롯 폭보다 넓다(좁은 플롯).
    const long = 'X'.repeat(40);
    const { placements } = layoutMarkerLabels([m(15, long)], 30, 400);
    expect(placements[0].align).toBe('right');
  });

  it('right 정렬 라벨이 플롯 좌측 밖으로 나가면 left로 유지한다', () => {
    // frac 0.9 > 0.6 → 초기 right지만 좁은 플롯에서 라벨이 좌측 0을 넘으면 좌측
    // 클리핑 — 그래도 우측보다 넘침이 작은 쪽을 고르므로 right 유지가 아닌
    // left 뒤집기는 anchor가 0에 가까울 때만 일어난다.
    const { placements } = layoutMarkerLabels([m(0, 'Y'.repeat(20))], 30, 720);
    expect(placements[0].align).toBe('left');
  });

  it('placements는 입력 순서를 보존한다(정렬은 내부에서만)', () => {
    const { placements } = layoutMarkerLabels([m(25, 'BB'), m(2, 'AA')], 30, 720);
    // 입력[0]=dayIdx25(우측·right), 입력[1]=dayIdx2(좌측·left)
    expect(placements[0].align).toBe('right');
    expect(placements[1].align).toBe('left');
  });

  it('3개 연속 인접이면 행 0·1·2 — gridTop은 최심 행 기준', () => {
    const { placements, gridTop } = layoutMarkerLabels(
      [m(10, 'A'.repeat(18)), m(11, 'B'.repeat(18)), m(12, 'C'.repeat(18))],
      30,
      720,
    );
    expect(placements.map((p) => p.row)).toEqual([0, 1, 2]);
    expect(gridTop).toBe(MARKER_BASE_TOP + 2 * MARKER_ROW_PX);
  });
});
