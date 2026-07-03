// 대시보드(B-1) — /v1/metrics series의 표시용 순수 로직.
//
// 코호트 원칙(테스트 SSOT: __tests__/seriesView.test.ts):
// - 경계는 "관측된 비어있지 않은 값이 달라질 때"만 만든다. fingerprint가 빈
//   세션(요약-only·OTLP-only 등)은 환경 변화의 증거가 아니므로 직전 코호트를
//   이어간다 — 가짜 경계 금지.
// - '<synthetic>'은 CC가 오류/주입 메시지에 쓰는 합성 모델 값이라 코호트
//   라벨에서 제외한다(실측: 대부분 세션에 실제 모델과 병존).
// - 판단(개선됐는가)은 여기 없다 — 정렬·구간화·라벨링만 한다(측정/판별 분리).
import type { SessionFingerprintDto, SessionSeriesRowDto } from '../api/types';

/** API는 최신-우선으로 주므로 시간축(과거→현재)용으로 오름차순 정렬한다.
 *  타임스탬프 동률은 session_id로 안정화. */
export function sortSeriesAscending(rows: SessionSeriesRowDto[]): SessionSeriesRowDto[] {
  return [...rows].sort((a, b) => {
    const t = a.first_observed_at.localeCompare(b.first_observed_at);
    return t !== 0 ? t : a.session_id.localeCompare(b.session_id);
  });
}

/** 코호트 라벨용 모델 목록 — '<synthetic>' 제외, 정렬 유지. */
export function cohortModels(fp: SessionFingerprintDto): string[] {
  return fp.models.filter((m) => m !== '<synthetic>');
}

export type CohortSegment = {
  /** 세션 축 인덱스(닫힌 구간). */
  start: number;
  end: number;
  /** 정렬된 값들의 ' + ' 결합. known=false면 ''. */
  label: string;
  /** 구간 안에서 값이 한 번이라도 관측됐는가 — 선행 무관측 구간만 false. */
  known: boolean;
};

/** 연속 세션을 같은 코호트 구간으로 묶는다. `pick`이 빈 배열을 주는 세션은
 *  직전 구간에 흡수된다(변화의 증거 아님). 선행 무관측 구간은 known=false. */
export function cohortSegments(
  rows: SessionSeriesRowDto[],
  pick: (row: SessionSeriesRowDto) => string[],
): CohortSegment[] {
  const segments: CohortSegment[] = [];
  let current: CohortSegment | null = null;
  rows.forEach((row, i) => {
    const values = [...pick(row)].sort();
    const label = values.join(' + ');
    if (values.length === 0) {
      if (current) {
        current.end = i;
      } else {
        current = { start: i, end: i, label: '', known: false };
      }
      return;
    }
    if (current && current.known && current.label === label) {
      current.end = i;
      return;
    }
    if (current) segments.push(current);
    current = { start: i, end: i, label, known: true };
  });
  if (current) segments.push(current);
  return segments;
}

export type CohortBoundary = {
  /** 새 코호트가 시작하는 세션 축 인덱스. */
  index: number;
  from: string;
  to: string;
};

/** known→known 구간 전환만 경계다 — 무관측 선행 구간에서 경계를 지어내지
 *  않는다. */
export function cohortBoundaries(segments: CohortSegment[]): CohortBoundary[] {
  const out: CohortBoundary[] = [];
  for (let i = 1; i < segments.length; i++) {
    const prev = segments[i - 1];
    const next = segments[i];
    if (prev.known && next.known) {
      out.push({ index: next.start, from: prev.label, to: next.label });
    }
  }
  return out;
}
