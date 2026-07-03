// 대시보드 차트 공유 기하 — 모든 블록(코호트 레일·outcome·metric strip)이
// 같은 세션 축을 쓴다: n개 등폭 트랙 + GAP_PX 간격. 경계 오버레이가 블록을
// 관통해 정렬되려면 모든 블록이 이 계산을 그대로 써야 한다.

export const GAP_PX = 2;

/** i번째 트랙의 왼쪽 edge — CSS calc 문자열. */
export function trackLeft(i: number, n: number): string {
  if (n <= 0) return '0%';
  return `calc((100% - ${(n - 1) * GAP_PX}px) * ${i} / ${n} + ${i * GAP_PX}px)`;
}

/** grid-template-columns 값. */
export function gridColumns(n: number): string {
  return `repeat(${Math.max(n, 1)}, 1fr)`;
}
