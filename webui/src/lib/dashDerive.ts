// 대시보드 전면 개편(2026-07-04)의 파생 SSOT — 스펙 docs/specs/2026-07-04-dashboard-redesign.md.
// 전부 순수 함수: 같은 rows(시간 오름차순) → 같은 결과. 판정 문장은 만들지
// 않는다 — 숫자·결정론 문구(관측 사실)만.
import type { SessionMetricsDto, SessionSeriesRowDto } from '../api/types';
import { cohortBoundaries, cohortModels, cohortSegments, displayModel } from './seriesView';

const STRIP_KEYS = [
  'tool_failure_count',
  'context_bloat_count',
  'api_error_count',
  'user_interruption_count',
  'compact_boundary_count',
  'tool_result_truncated_count',
] as const;

export function signalsOf(m: SessionMetricsDto): number {
  return STRIP_KEYS.reduce((a, k) => a + (m[k] ?? 0), 0);
}

const dayKey = (iso: string) => iso.slice(5, 10); // 'MM-DD'
const dayOrdinal = (iso: string) => Math.floor(Date.parse(iso.slice(0, 10)) / 86_400_000);

export type Daily = {
  dates: string[];
  cost: number[];
  signals: number[];
  passed: number[];
  failed: number[];
  unknown: number[];
  /** 날짜 버킷 → rows 인덱스 목록(툴팁의 그날 세션 목록용). */
  sessionsOf: number[][];
};

/** 첫 세션 날짜부터 마지막까지 연속 일자 버킷 — 빈 날을 0으로 채워 시간축이
 *  왜곡되지 않게 한다. */
export function buildDaily(rows: SessionSeriesRowDto[]): Daily {
  if (rows.length === 0)
    return { dates: [], cost: [], signals: [], passed: [], failed: [], unknown: [], sessionsOf: [] };
  const ord0 = dayOrdinal(rows[0].first_observed_at);
  const ordN = dayOrdinal(rows[rows.length - 1].first_observed_at);
  const n = ordN - ord0 + 1;
  const z = () => Array(n).fill(0) as number[];
  const d: Daily = {
    dates: Array.from({ length: n }, (_, i) =>
      new Date((ord0 + i) * 86_400_000).toISOString().slice(5, 10),
    ),
    cost: z(),
    signals: z(),
    passed: z(),
    failed: z(),
    unknown: z(),
    sessionsOf: Array.from({ length: n }, () => []),
  };
  rows.forEach((r, ri) => {
    const i = dayOrdinal(r.first_observed_at) - ord0;
    if (i < 0 || i >= n) return;
    d.cost[i] += r.metrics.estimated_cost_usd ?? 0;
    d.signals[i] += signalsOf(r.metrics);
    d.passed[i] += r.metrics.verification_passed;
    d.failed[i] += r.metrics.verification_failed;
    d.unknown[i] += r.metrics.verification_unknown;
    d.sessionsOf[i].push(ri);
  });
  return d;
}

export type Headline = {
  sessions: number;
  events: number;
  passRatePct: number | null;
  cost: number;
  unitRatePerM: number | null;
  cacheHitPct: number | null;
  toolFailPct: number | null;
  toolFails: number;
  toolCalls: number;
  guards: number;
};

const round1 = (v: number) => Math.round(v * 10) / 10;

/** 창 요약 — 비율은 전부 측정 합계 기반, 분모 0이면 null(0으로 위장 금지). */
export function headline(rows: SessionSeriesRowDto[]): Headline {
  let passed = 0, failed = 0, cost = 0, billed = 0, read = 0, ctxDenom = 0;
  let toolCalls = 0, toolFails = 0, events = 0, guards = 0;
  for (const r of rows) {
    const m = r.metrics;
    passed += m.verification_passed;
    failed += m.verification_failed;
    guards += m.verification_total;
    cost += m.estimated_cost_usd ?? 0;
    const input = m.input_tokens ?? 0;
    const creation = m.cache_creation_input_tokens ?? 0;
    const rd = m.cache_read_input_tokens ?? 0;
    billed += input + creation + (m.output_tokens ?? 0);
    read += rd;
    ctxDenom += input + creation + rd;
    toolCalls += m.tool_call_total;
    toolFails += m.tool_failure_count;
    events += r.event_count;
  }
  return {
    sessions: rows.length,
    events,
    guards,
    passRatePct: passed + failed > 0 ? round1((passed / (passed + failed)) * 100) : null,
    cost: Math.round(cost * 100) / 100,
    unitRatePerM: billed > 0 ? Math.round((cost / billed) * 1e6 * 100) / 100 : null,
    cacheHitPct: ctxDenom > 0 ? round1((read / ctxDenom) * 100) : null,
    toolFailPct: toolCalls > 0 ? round1((toolFails / toolCalls) * 100) : null,
    toolFails,
    toolCalls,
  };
}

export type HeadlineDelta = {
  passRatePp: number | null;
  cost: number | null;
  unitRate: number | null;
  cacheHitPp: number | null;
  toolFailPp: number | null;
};

export function headlineDelta(cur: Headline, prev: Headline | null): HeadlineDelta {
  const d = (a: number | null, b: number | null) =>
    a !== null && b !== null ? Math.round((a - b) * 100) / 100 : null;
  if (!prev) return { passRatePp: null, cost: null, unitRate: null, cacheHitPp: null, toolFailPp: null };
  return {
    passRatePp: d(cur.passRatePct, prev.passRatePct),
    cost: d(cur.cost, prev.cost),
    unitRate: d(cur.unitRatePerM, prev.unitRatePerM),
    cacheHitPp: d(cur.cacheHitPct, prev.cacheHitPct),
    toolFailPp: d(cur.toolFailPct, prev.toolFailPct),
  };
}

export type ObservedChange =
  | { kind: 'model_first'; date: string; model: string }
  | { kind: 'cc_change'; date: string; from: string; to: string }
  | { kind: 'top_signals'; sessionId: string; n: number };

/** "관측된 변화" — 전부 결정론: 창 내 첫 관측 모델, CC 전환, 신호 최다 세션.
 *  문구는 만들지 않는다 — 구조만 반환하고 번역은 i18n 카탈로그가 한다. */
export function observedChanges(rows: SessionSeriesRowDto[]): ObservedChange[] {
  const out: ObservedChange[] = [];
  const seen = new Set<string>();
  rows.forEach((r, i) => {
    for (const m of cohortModels(r.fingerprint)) {
      if (!seen.has(m)) {
        seen.add(m);
        if (i > 0)
          out.push({ kind: 'model_first', date: dayKey(r.first_observed_at), model: displayModel(m) });
      }
    }
  });
  const ccSegs = cohortSegments(rows, (r) => r.fingerprint.cc_versions);
  for (const b of cohortBoundaries(ccSegs)) {
    out.push({ kind: 'cc_change', date: dayKey(rows[b.index].first_observed_at), from: b.from, to: b.to });
  }
  let maxI = -1;
  let maxV = 0;
  rows.forEach((r, i) => {
    const s = signalsOf(r.metrics);
    if (s > maxV) {
      maxV = s;
      maxI = i;
    }
  });
  if (maxI >= 0 && maxV > 0)
    out.push({ kind: 'top_signals', sessionId: rows[maxI].session_id, n: maxV });
  return out;
}

export type CohortAgg = {
  n: number;
  unitRatePerM: number | null;
  passRatePct: number | null;
  signalsPerSession: number;
  cacheHitPct: number | null;
};

export type CohortCompare = {
  label: string;
  boundaryIdx: number;
  alsoCcChanged: boolean;
  before: CohortAgg;
  after: CohortAgg;
  lowSample: boolean;
};

function agg(rows: SessionSeriesRowDto[]): CohortAgg {
  const h = headline(rows);
  const signals = rows.reduce((a, r) => a + signalsOf(r.metrics), 0);
  return {
    n: rows.length,
    unitRatePerM: h.unitRatePerM,
    passRatePct: h.passRatePct,
    signalsPerSession: rows.length > 0 ? round1(signals / rows.length) : 0,
    cacheHitPct: h.cacheHitPct,
  };
}

/** 최신 모델-집합 경계의 인접 세그먼트 비교. 라벨은 집합 diff에서 파생:
 *  유입만 → "{names} 유입", 이탈만 → "{names} 이탈", 둘 다 → "A → B". */
export function cohortCompare(rows: SessionSeriesRowDto[]): CohortCompare | null {
  const segs = cohortSegments(rows, (r) => cohortModels(r.fingerprint));
  const known = segs.filter((s) => s.known);
  const bs = cohortBoundaries(segs);
  if (bs.length === 0) return null;
  const b = bs[bs.length - 1];
  const segAfter = known.find((s) => s.start === b.index)!;
  const segBefore = [...known].reverse().find((s) => s.start < b.index)!;
  const setOf = (label: string) => new Set(label.split(' + ').filter(Boolean));
  const beforeSet = setOf(b.from);
  const afterSet = setOf(b.to);
  const added = [...afterSet].filter((m) => !beforeSet.has(m)).map(displayModel);
  const removed = [...beforeSet].filter((m) => !afterSet.has(m)).map(displayModel);
  const label =
    added.length && removed.length
      ? `${removed.join(' · ')} → ${added.join(' · ')}`
      : added.length
        ? `${added.join(' · ')} 유입`
        : `${removed.join(' · ')} 이탈`;
  const beforeRows = rows.slice(segBefore.start, segBefore.end + 1);
  const afterRows = rows.slice(segAfter.start, segAfter.end + 1);
  const ccAt = (idx: number): string | null => {
    for (let i = idx; i >= 0; i--) {
      const cc = rows[i].fingerprint.cc_versions;
      if (cc.length > 0) return cc.join(' + ');
    }
    return null;
  };
  const ccBefore = ccAt(b.index - 1);
  const ccAfterRow = rows[b.index].fingerprint.cc_versions;
  const alsoCcChanged =
    ccAfterRow.length > 0 && ccBefore !== null && ccAfterRow.join(' + ') !== ccBefore;
  return {
    label,
    boundaryIdx: b.index,
    alsoCcChanged,
    before: agg(beforeRows),
    after: agg(afterRows),
    lowSample: Math.min(beforeRows.length, afterRows.length) < 3,
  };
}

/** 카드 레인 greedy 배치 — 왼쪽부터, 겹치면 다음 레인, 빈 레인 재사용.
 *  items는 x 오름차순 가정. 반환은 item별 레인 인덱스. */
export function laneLayout(items: Array<{ x: number }>, cardWidthPct: number): number[] {
  const laneEnds: number[] = [];
  return items.map(({ x }) => {
    let li = laneEnds.findIndex((end) => x >= end);
    if (li < 0) {
      li = laneEnds.length;
      laneEnds.push(0);
    }
    laneEnds[li] = x + cardWidthPct;
    return li;
  });
}
