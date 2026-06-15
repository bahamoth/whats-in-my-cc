/**
 * Pure view-model builder for the insight strip (design spec §3/§5).
 * Turns the already-fetched query DTOs into typed cards with provenance and
 * drill payloads. ALL derivation logic lives here so it is unit-testable in
 * jsdom; the component only renders. Degrades gracefully: when a backend slice
 * has not landed, the relevant card is badged `uncollected` (미수집·예정).
 *
 * Consumes the REAL APIs:
 *  - VerificationRunDto.detection_basis / status_basis drive the 검증
 *    card's badge (measured when all runs are known_tool + exit, else mixed).
 *  - SignalDto(detector=tool_failure) count drives the 도구 실패 card
 *    (deterministic L1 extractor count).
 *  - SessionUsageDto.estimated_cost_usd / cost_basis is the cost 추정.
 *  - An optional cross-session baseline (baseline cache_hit_ratio — NOT the
 *    removed per-session scalar) renders a "vs median" delta.
 */
import type {
  SessionUsageDto,
  VerificationRunDto,
  SignalDto,
  TurnRollupDto,
} from '../../../api/types';
import { formatPct, formatTokens, formatUsd } from '../../../lib/format';
import type { Provenance } from './provenance';

/** Optional cross-session baseline (slice 6 + S8). Absent fields → no delta. */
export interface InsightBaseline {
  cache_hit_ratio?: number | null;
  billed_tokens?: number | null;
}

export interface InsightInputs {
  usage: SessionUsageDto | undefined;
  verificationRuns: VerificationRunDto[] | undefined;
  signals: SignalDto[] | undefined;
  baseline?: InsightBaseline;
  /** S8 — per-turn rollup; drives intra-session sparklines (token-derived
   *  cards only — context / tokens / cost). Absent → no sparkline. */
  turns?: TurnRollupDto[];
}

/** S8 — fact-only sparkline tints (design §6.3): 맥락·비용=blue. */
export type SparklineTint = 'green' | 'amber' | 'blue';

export type InsightCardId =
  | 'context'
  | 'tokens'
  | 'verification'
  | 'tool_failure'
  | 'cost';

export interface InsightCardModel {
  id: InsightCardId;
  /** Korean card title shown in the strip. */
  title: string;
  /** Headline value, already formatted; `—` when uncollected. */
  value: string;
  /** One-line micro-detail under the value. */
  detail: string;
  provenance: Provenance;
  /** Long-form text for the `?` tooltip. */
  tooltip: string;
  /** Inline drill content shown when the card is expanded. */
  drill?: {
    lines: string[];
    byKind?: Record<string, number>;
  };
  /** Optional "vs your median" delta (slice 6); undefined when no baseline. */
  baselineDelta?: string;
  /** S8 — per-turn trend (raw values; the strip normalises to bar heights).
   *  Undefined when no per-turn data is available. */
  sparkline?: number[];
  /** S8 — fact-only tint for the sparkline. */
  sparklineTint?: SparklineTint;
}

/** S8 — per-turn cache-hit ratio = cache_read / (cache_read + cache_creation +
 *  input); turns whose denominator is 0 contribute 0. Mirrors the card metric. */
function perTurnCacheHit(turns: TurnRollupDto[]): number[] | undefined {
  const series = turns
    .filter((t) => t.tokens)
    .map((t) => {
      const k = t.tokens!;
      const denom = k.cache_read_input_tokens + k.cache_creation_input_tokens + k.input_tokens;
      return denom > 0 ? k.cache_read_input_tokens / denom : 0;
    });
  return series.length > 0 ? series : undefined;
}

/** S8 — per-turn billed tokens = input + cache_creation + output (cache_read is
 *  free, never billed). Drives both the tokens and (proxy) cost sparklines. */
function perTurnBilled(turns: TurnRollupDto[]): number[] | undefined {
  const series = turns
    .filter((t) => t.tokens)
    .map((t) => {
      const k = t.tokens!;
      return k.input_tokens + k.cache_creation_input_tokens + k.output_tokens;
    });
  return series.length > 0 ? series : undefined;
}

// /usage 토큰 component에서 cache hit ratio 계산(window=세션 전체)
function cacheHitRatio(u: SessionUsageDto): number | null {
  const denom = u.cache_read_input_tokens + u.cache_creation_input_tokens + u.input_tokens;
  return denom > 0 ? u.cache_read_input_tokens / denom : null;
}

const GUARD_KIND: Record<string, 'test' | 'build' | 'lint' | 'format'> = {
  test_suite_js: 'test',
  test_suite_rust: 'test',
  test_suite_py: 'test',
  test_suite_go: 'test',
  test_suite_java: 'test',
  build: 'build',
  build_check: 'build',
  lint: 'lint',
  format_check: 'format',
};

function contextCard(inputs: InsightInputs): InsightCardModel {
  const tip =
    '캐시 적중률 = cache_read / (cache_read + cache_creation + input). 측정값(usage facet). ' +
    '고정 캐시 컨텍스트 크기·증가·캐시 미스는 펼쳐서 확인. 시스템 프롬프트/스킬/메모리 단위 분해와 ' +
    '"오염" 판정은 데이터에 없어 제공하지 않습니다(설계 §8 한계).';
  if (!inputs.usage) {
    return {
      id: 'context', title: '컨텍스트 효율', value: '—',
      detail: 'usage facet 재수집 필요', provenance: 'uncollected', tooltip: tip,
    };
  }
  const u = inputs.usage;
  const ratio = cacheHitRatio(u);
  const card: InsightCardModel = {
    id: 'context', title: '컨텍스트 효율',
    value: formatPct(ratio),
    detail: `캐시 읽기 ${formatTokens(u.cache_read_input_tokens)}`,
    provenance: 'measured', tooltip: tip,
    drill: {
      lines: [
        `캐시 적중률 ${formatPct(ratio)}`,
        `캐시 읽기(무료) ${formatTokens(u.cache_read_input_tokens)}`,
        `캐시 생성 ${formatTokens(u.cache_creation_input_tokens)}`,
        `사용자 턴 ${u.user_turns}`,
      ],
    },
  };
  const base = inputs.baseline?.cache_hit_ratio;
  if (typeof base === 'number' && typeof ratio === 'number') {
    const d = Math.round((ratio - base) * 100);
    card.baselineDelta = `${d >= 0 ? '+' : ''}${d}%p vs 중앙값`;
  }
  if (inputs.turns) {
    const spark = perTurnCacheHit(inputs.turns);
    if (spark) {
      card.sparkline = spark;
      card.sparklineTint = 'blue';
    }
  }
  return card;
}

function tokensCard(inputs: InsightInputs): InsightCardModel {
  const tip =
    '청구 토큰(input + cache_creation + output)과 캐시 읽기(무료)는 의미가 달라 절대 합산하지 않습니다 ' +
    '(설계 §3 Q2). 측정값(usage facet).';
  if (!inputs.usage) {
    return {
      id: 'tokens', title: '토큰', value: '—',
      detail: 'usage facet 재수집 필요', provenance: 'uncollected', tooltip: tip,
    };
  }
  const u = inputs.usage;
  const card: InsightCardModel = {
    id: 'tokens', title: '토큰',
    value: `청구 ${formatTokens(u.billed_tokens)}`,
    detail: `캐시 읽기 ${formatTokens(u.cache_read_input_tokens)} (무료)`,
    provenance: 'measured', tooltip: tip,
    drill: {
      lines: [
        `input ${formatTokens(u.input_tokens)}`,
        `cache_creation ${formatTokens(u.cache_creation_input_tokens)}`,
        `output ${formatTokens(u.output_tokens)}`,
        ...u.by_model.map((m) => `${m.model}: ${m.assistant_events} 산출 · 출력 ${formatTokens(m.output_tokens)}`),
      ],
    },
  };
  const baseMedian = inputs.baseline?.billed_tokens;
  if (typeof baseMedian === 'number' && baseMedian > 0) {
    const d = Math.round((u.billed_tokens / baseMedian - 1) * 100);
    card.baselineDelta = `${d >= 0 ? '+' : ''}${d}% vs 중앙값`;
  }
  if (inputs.turns) {
    const spark = perTurnBilled(inputs.turns);
    if (spark) {
      card.sparkline = spark;
      card.sparklineTint = 'blue';
    }
  }
  return card;
}

function verificationCard(inputs: InsightInputs): InsightCardModel {
  const tip =
    '가드 = 실행된 테스트/빌드/린트/포맷 검사. 알려진 도구 매칭(known_tool) + 종료코드(exit) 기반이면 측정, ' +
    '파이프(piped)로 가려진 종료코드가 섞이면 혼합으로 표시(슬라이스 2 detection_basis/status_basis). ' +
    '키워드 추정(test_keyword)은 더 이상 생성되지 않으며(F2), 과거 ingest된 older 데이터에만 나타날 수 있습니다. ' +
    '브라우저 스모크/서브에이전트 테스트는 감지하지 않습니다(설계 §3 Q4 한계).';
  const runs = inputs.verificationRuns;
  if (!runs || runs.length === 0) {
    return {
      id: 'verification', title: '검증', value: '—',
      detail: runs ? '감지된 가드 없음' : '로딩 중',
      provenance: 'uncollected', tooltip: tip,
    };
  }
  const byKind: Record<string, number> = {};
  let passed = 0;
  let failed = 0;
  let unknown = 0;
  let allMeasured = true;
  for (const r of runs) {
    const k = GUARD_KIND[r.command_kind] ?? 'test';
    byKind[k] = (byKind[k] ?? 0) + 1;
    if (r.status === 'passed') passed += 1;
    else if (r.status === 'failed') failed += 1;
    else if (r.status === 'unknown') unknown += 1;
    // slice-2 fields: measured only when every run is a known-tool match with a
    // direct exit-code status. Keyword guesses or piped (masked) exits → mixed.
    if (r.detection_basis !== 'known_tool' || r.status_basis !== 'exit') {
      allMeasured = false;
    }
  }
  // Pass rate is over MEASURED runs (passed+failed), NOT the total — most guards
  // can be unmeasured (piped, no exit). Surface that denominator + the unmeasured
  // count so "통과 N" can't be misread against the total (dogfooding 2026-06-11).
  const measured = passed + failed;
  const detailParts = Object.entries(byKind).map(([k, n]) => `${k} ${n}`);
  if (unknown > 0) detailParts.push(`미측정 ${unknown}`);
  return {
    id: 'verification', title: '검증',
    value: measured > 0 ? `가드 ${runs.length} · 통과 ${passed}/${measured}` : `가드 ${runs.length} · 측정 없음`,
    detail: detailParts.join(' · '),
    provenance: allMeasured ? 'measured' : 'mixed', tooltip: tip,
    drill: {
      // status_provenance (0022) = how each run's STATUS was determined — a
      // per-run axis distinct from the card badge (measured/mixed above, which
      // derives from detection_basis/status_basis). Only 'estimated' (output-
      // text heuristic instead of an exit code) is flagged; measured/unknown/
      // null (pre-0022 rows) render unmarked, matching the badge convention of
      // calling out only the degraded case.
      lines: runs.map(
        (r) =>
          `${r.command_kind} → ${r.status}${r.status_provenance === 'estimated' ? ' (추정)' : ''}`,
      ),
      byKind,
    },
  };
}

function toolFailureCard(inputs: InsightInputs): InsightCardModel {
  const tip = '도구 실패 signal 수(detector=tool_failure). 결정적 카운트이며 심각도 판단은 포함하지 않습니다.';
  const sigs = inputs.signals;
  if (!sigs) {
    return { id: 'tool_failure', title: '도구 실패', value: '—', detail: '로딩 중', provenance: 'uncollected', tooltip: tip };
  }
  const failures = sigs.filter((s) => s.detector === 'tool_failure');
  return {
    id: 'tool_failure', title: '도구 실패',
    value: `${failures.length}`,
    detail: failures.length === 0 ? '도구 실패 없음' : '펼쳐서 확인',
    provenance: 'measured', tooltip: tip,
    drill: { lines: failures.map((s) => `${s.subkind ?? s.detector} · ${s.summary}`) },
  };
}

function costCard(inputs: InsightInputs): InsightCardModel {
  const tip =
    '공개 가격표 × usage 토큰으로 계산한 추정치이며 실제 청구액이 아닙니다(설계 §6.5/§11.3). ' +
    'OTel claude_code.cost.usage 메트릭이 들어오면 대체됩니다. cache_read(무료)는 비용에서 제외.';
  if (!inputs.usage) {
    return {
      id: 'cost', title: '비용', value: '—',
      detail: 'usage facet 재수집 필요', provenance: 'uncollected', tooltip: tip,
    };
  }
  const u = inputs.usage;
  const unpriced = u.models_without_pricing.length > 0;
  const card: InsightCardModel = {
    id: 'cost', title: '비용',
    value: formatUsd(u.estimated_cost_usd),
    detail: unpriced
      ? `공개 가격표 추정 (≈) · 미가격 ${u.models_without_pricing.length}`
      : '공개 가격표 추정 (≈)',
    provenance: 'estimated', tooltip: tip,
    drill: {
      lines: u.by_model.map(
        (m) => `${m.model}: ${m.priced ? formatUsd(m.estimated_cost_usd) : '가격표 없음'}`,
      ),
    },
  };
  // S8 — cost has no per-turn pricing breakdown; bill ∝ cost, so the per-turn
  // billed-tokens series is an honest *shape* proxy for the cost trend.
  if (inputs.turns) {
    const spark = perTurnBilled(inputs.turns);
    if (spark) {
      card.sparkline = spark;
      card.sparklineTint = 'blue';
    }
  }
  return card;
}

export function buildInsightCards(inputs: InsightInputs): InsightCardModel[] {
  return [
    contextCard(inputs),
    tokensCard(inputs),
    verificationCard(inputs),
    toolFailureCard(inputs),
    costCard(inputs),
  ];
}
