// PR-3 §3d — DetailPanel 요청 메트릭 행의 "세션 중앙값의 x.x×" 배지 파생.
// 백엔드 전수 p50(SessionMetrics.llm_request_p50) 기준 — 로드 윈도우 근사 아님.
import type { P50StatDto } from '../../../api/types';
import type { TFunction } from '../../../i18n';

export type P50Badge = { text: string; lowSample: boolean };

export function p50Badge(
  value: number | null,
  stat: P50StatDto | undefined,
  t: TFunction,
): P50Badge | null {
  if (value == null || !stat) return null;
  if (stat.n < 3) return { text: t('metric.badge.lowSample'), lowSample: true };
  if (stat.p50 == null || stat.p50 <= 0) return null;
  return { text: t('metric.badge.median', (value / stat.p50).toFixed(1)), lowSample: false };
}
