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
  /** CC 전환 요약 — 실데이터에서 CC는 자주 바뀌므로(창 내 수 회) 항목을
   *  나열하지 않고 처음→마지막 + 전환 횟수로 압축한다. lastDate는 마지막
   *  전환 세션의 날짜(시간축 마커 위치). */
  | { kind: 'cc_span'; from: string; to: string; count: number; lastDate: string }
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
  const ccKnown = ccSegs.filter((s) => s.known);
  const ccBs = cohortBoundaries(ccSegs);
  if (ccBs.length > 0) {
    out.push({
      kind: 'cc_span',
      from: ccKnown[0].label.split(' + ')[0],
      to: ccKnown[ccKnown.length - 1].label.split(' + ').at(-1)!,
      count: ccBs.length,
      lastDate: dayKey(rows[ccBs[ccBs.length - 1].index].first_observed_at),
    });
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

/** 세션 묶음의 코호트 집계 — headline 합계 기반(분모 0 → null). */
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

/** 모델 → 색 SSOT — 카드 레인·스캐터가 공유. 최초 관측순 고정 배정이라
 *  창을 바꿔도 살아남은 모델의 색이 흔들리지 않는다(색은 정체성을 따른다). */
const MODEL_PALETTE = ['#7da7ff', '#f0b429', '#b07dff', '#ff8a4c', '#2bd0d0', '#d97aff'];
export const MODEL_OVERFLOW_COLOR = '#48536b';

export function modelColors(rows: SessionSeriesRowDto[]): Map<string, string> {
  const map = new Map<string, string>();
  for (const r of rows) {
    for (const m of cohortModels(r.fingerprint)) {
      if (!map.has(m)) {
        map.set(m, MODEL_PALETTE[map.size] ?? MODEL_OVERFLOW_COLOR);
      }
    }
  }
  return map;
}

/* ── 코호트 경계 랭킹 (스펙 §2 3차 개정) ─────────────────────────────
 * 도구는 차원을 고르지 않는다: fingerprint 5차원 전부에서 경계를 검출하고,
 * "유의"는 결정론 통계(임의 분할 대비 초과율)로만 정한다. 상수 3개의 SSOT는
 * 스펙 §2 — detector 임계값과 같은 지위. */
export const COHORT_MIN_N = 3;
export const COHORT_EXCEED_MAX = 0.1;
export const COHORT_TOP_K = 3;

export type CohortDim = 'models' | 'cc' | 'branch' | 'cwd' | 'entrypoint' | 'plugins' | 'instructions';

/** 개입 차원 — 사용자가 교체할 수 있는 실행 환경(독립변수). 경계 비교·랭킹 대상.
 *  근거: fingerprint 설계 의도("자기개선 루프의 독립변수 표면"), 스펙 §2 4차 개정. */
export const INTERVENTION_DIMS: CohortDim[] = ['models', 'cc', 'entrypoint', 'plugins', 'instructions'];
/** 맥락 차원 — 작업 내용의 식별자(교란 변수). 비교 대상이 아니라 각주 전용:
 *  branch 전환은 "환경 변화"가 아니라 "다른 작업 시작"이고, cwd는 프로젝트
 *  필터와 중복이며 프로젝트 간 지표 비교는 성립하지 않는다. */
export const CONTEXT_DIMS: CohortDim[] = ['branch', 'cwd'];
export const COHORT_DIMS: CohortDim[] = [...INTERVENTION_DIMS, ...CONTEXT_DIMS];

const DIM_PICK: Record<CohortDim, (r: SessionSeriesRowDto) => string[]> = {
  models: (r) => cohortModels(r.fingerprint),
  cc: (r) => r.fingerprint.cc_versions,
  branch: (r) => r.fingerprint.git_branches,
  cwd: (r) => r.fingerprint.cwds,
  entrypoint: (r) => r.fingerprint.entrypoints,
  plugins: (r) => r.fingerprint.plugins ?? [],
  // 전체 해시를 값으로 유지(diff 조회 키) — 표시 축약은 rankCohorts의 fmt가 한다.
  instructions: (r) => (r.fingerprint.instructions ?? []).map((x) => `${x.source}:${x.hash}`),
};

/** 표시용 값 포맷 — instructions는 'source:hash8'로 축약, 그 외는 원문/모델명. */
function fmtDimValue(dim: CohortDim, v: string): string {
  if (dim === 'models') return displayModel(v);
  if (dim === 'instructions') {
    const i = v.indexOf(':');
    return i >= 0 ? `${v.slice(0, i)}:${v.slice(i + 1, i + 9)}` : v;
  }
  return v;
}

export type CohortMetric = 'unitRate' | 'passRate' | 'signals' | 'cacheHit';
const METRIC_ORDER: CohortMetric[] = ['unitRate', 'passRate', 'signals', 'cacheHit'];

function metricOf(a: CohortAgg, m: CohortMetric): number | null {
  switch (m) {
    case 'unitRate':
      return a.unitRatePerM;
    case 'passRate':
      return a.passRatePct;
    case 'signals':
      return a.signalsPerSession;
    case 'cacheHit':
      return a.cacheHitPct;
  }
}

export type RankedBoundary = {
  dim: CohortDim;
  /** after가 시작하는 rows 인덱스. */
  index: number;
  date: string;
  added: string[];
  removed: string[];
  /** 표시 축약 전 원본 값(예: instructions의 전체 해시) — diff 조회 키. */
  addedRaw: string[];
  removedRaw: string[];
  before: CohortAgg;
  after: CohortAgg;
  /** 초과율이 가장 낮은(가장 두드러진) 지표와 그 |Δ|·초과율. 표본 게이트
   *  미달이거나 지표 계산 불가면 null — "표본 부족" 표기용. */
  bestMetric: CohortMetric | null;
  bestDelta: number | null;
  exceed: number | null;
  /** 같은 인덱스에서 함께 변한 다른 차원(효과 분리 불가 각주). */
  alsoChanged: CohortDim[];
};

/** 창의 모든 유효 분할점 통계(prefix/suffix |Δ|)를 지표별로 전수 계산. */
function splitStats(rows: SessionSeriesRowDto[]): Map<CohortMetric, Map<number, number>> {
  const out = new Map<CohortMetric, Map<number, number>>(METRIC_ORDER.map((m) => [m, new Map()]));
  for (let k = COHORT_MIN_N; k <= rows.length - COHORT_MIN_N; k++) {
    const L = agg(rows.slice(0, k));
    const R = agg(rows.slice(k));
    for (const m of METRIC_ORDER) {
      const a = metricOf(L, m);
      const b = metricOf(R, m);
      if (a !== null && b !== null) out.get(m)!.set(k, Math.abs(b - a));
    }
  }
  return out;
}

export function rankCohorts(rows: SessionSeriesRowDto[]): {
  surfaced: RankedBoundary[];
  all: RankedBoundary[];
  /** 다중비교 보정 후 유효 임계(B-14) — EXCEED_MAX ÷ 활성 개입 차원 수.
   *  차원이 늘수록 "임의 상위 10%"에 우연히 걸릴 기회도 늘기 때문. */
  effectiveExceedMax: number;
} {
  if (rows.length === 0) return { surfaced: [], all: [], effectiveExceedMax: COHORT_EXCEED_MAX };
  const stats = splitStats(rows);
  const dimBoundaries = new Map<CohortDim, Set<number>>();
  const all: RankedBoundary[] = [];

  for (const dim of COHORT_DIMS) {
    const segs = cohortSegments(rows, DIM_PICK[dim]);
    const bs = cohortBoundaries(segs);
    dimBoundaries.set(dim, new Set(bs.map((b) => b.index)));
    // 맥락 차원(branch·cwd)은 각주(alsoChanged) 계산에만 참여한다.
    if (!INTERVENTION_DIMS.includes(dim)) continue;
    for (const b of bs) {
      const setOf = (label: string) => new Set(label.split(' + ').filter(Boolean));
      const beforeSet = setOf(b.from);
      const afterSet = setOf(b.to);
      const addedRaw = [...afterSet].filter((v) => !beforeSet.has(v));
      const removedRaw = [...beforeSet].filter((v) => !afterSet.has(v));
      const fmt = (v: string) => fmtDimValue(dim, v);
      const before = agg(rows.slice(0, b.index));
      const after = agg(rows.slice(b.index));
      const gated = b.index >= COHORT_MIN_N && rows.length - b.index >= COHORT_MIN_N;
      let bestMetric: CohortMetric | null = null;
      let bestDelta: number | null = null;
      let exceed: number | null = null;
      if (gated) {
        for (const m of METRIC_ORDER) {
          const perK = stats.get(m)!;
          const mine = perK.get(b.index);
          if (mine === undefined || perK.size === 0) continue;
          // 자기 제외 초과율 — 자기 포함이면 분할 수가 적은 창에서 최솟값이
          // 1/#k로 바닥나 임계(0.10)를 구조적으로 못 넘는다(스펙 §2).
          let ge = 0;
          for (const [k, v] of perK) if (k !== b.index && v >= mine) ge++;
          const ex = ge / perK.size;
          if (exceed === null || ex < exceed) {
            exceed = ex;
            bestMetric = m;
            bestDelta = Math.round(mine * 100) / 100;
          }
        }
      }
      all.push({
        dim,
        index: b.index,
        date: rows[b.index].first_observed_at.slice(5, 10),
        added: addedRaw.map(fmt),
        removed: removedRaw.map(fmt),
        addedRaw,
        removedRaw,
        before,
        after,
        bestMetric,
        bestDelta,
        exceed,
        alsoChanged: [],
      });
    }
  }
  for (const b of all) {
    b.alsoChanged = COHORT_DIMS.filter(
      (d) => d !== b.dim && dimBoundaries.get(d)!.has(b.index),
    );
  }
  // B-14: 게이트 통과 후보를 보유한 개입 차원 수로 임계를 나눈다(Bonferroni류).
  const activeDims = new Set(all.filter((b) => b.exceed !== null).map((b) => b.dim)).size;
  const effectiveExceedMax = COHORT_EXCEED_MAX / Math.max(1, activeDims);
  const surfaced = all
    .filter((b) => b.exceed !== null && b.exceed <= effectiveExceedMax)
    .sort((a, b) => a.exceed! - b.exceed! || b.index - a.index)
    .slice(0, COHORT_TOP_K);
  return { surfaced, all, effectiveExceedMax };
}
