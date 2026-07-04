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

/** 모델 결정론 약칭 (2026-07-04 대시보드) — 좁은 레일 세그먼트에서도 읽히게.
 *  'haiku-4-5-20251001' → 'H4.5', 'opus-4-8' → 'O4.8', 'fable-5' → 'F5'.
 *  규칙: family 첫 글자 대문자 + major[.minor] (날짜 접미 제거). 패턴을
 *  벗어나는 이름은 원문 유지 — 약칭은 표시용이고 전체 이름은 title/툴팁이
 *  항상 보존한다. */
export function shortModel(name: string): string {
  const m = name.replace(/^claude-/, '').match(/^([a-z]+)-(\d+)(?:-(\d+))?(?:-\d{6,})?$/);
  if (!m) return name.replace(/^claude-/, '');
  const [, family, major, minor] = m;
  return family.charAt(0).toUpperCase() + major + (minor ? `.${minor}` : '');
}

/** 모델 집합 라벨 약칭 — 'a + b' 코호트 라벨을 'A1+B2'로. */
export function shortModelSet(label: string): string {
  return label
    .split(' + ')
    .map((x) => shortModel(x))
    .join('+');
}

/** 세션 usage 비율 파생 — 대시보드 효율 매트릭스의 SSOT.
 * cacheHitPct  = cache_read / (input + cache_creation + cache_read)
 * outSharePct  = output / 과금 토큰(input + cache_creation + output)
 * unitRatePerM = 추정 비용 ÷ 과금 토큰 × 1M (블렌디드 단가 — 비싼 모델 믹스일수록 높다)
 * usage facet이 아직 없는 세션은 measured=false로 구분한다(0과 미측정은 다르다). */
export function usageRatios(m: {
  input_tokens?: number;
  cache_creation_input_tokens?: number;
  cache_read_input_tokens?: number;
  output_tokens?: number;
  estimated_cost_usd?: number;
}): { cacheHitPct: number; outSharePct: number; unitRatePerM: number; measured: boolean } {
  const input = m.input_tokens ?? 0;
  const creation = m.cache_creation_input_tokens ?? 0;
  const read = m.cache_read_input_tokens ?? 0;
  const output = m.output_tokens ?? 0;
  const cost = m.estimated_cost_usd ?? 0;
  const billed = input + creation + output;
  const contextDenom = input + creation + read;
  const round1 = (v: number) => Math.round(v * 10) / 10;
  return {
    cacheHitPct: contextDenom > 0 ? round1((read / contextDenom) * 100) : 0,
    outSharePct: billed > 0 ? round1((output / billed) * 100) : 0,
    unitRatePerM: billed > 0 ? Math.round((cost / billed) * 1e6 * 100) / 100 : 0,
    measured: billed + read > 0,
  };
}

/** 모델 전체 표시명 (2026-07-04 전면 개편 — 약칭 UI 표기 금지 원칙).
 *  'claude-fable-5' → 'Fable 5', 'haiku-4-5-20251001' → 'Haiku 4.5'.
 *  패턴 밖 이름은 claude- 접두사만 벗겨 원문 유지. */
export function displayModel(name: string): string {
  const stripped = name.replace(/^claude-/, '');
  const m = stripped.match(/^([a-z]+)-(\d+)(?:-(\d+))?(?:-\d{6,})?$/);
  if (!m) return stripped;
  const [, family, major, minor] = m;
  return family.charAt(0).toUpperCase() + family.slice(1) + ' ' + major + (minor ? `.${minor}` : '');
}
