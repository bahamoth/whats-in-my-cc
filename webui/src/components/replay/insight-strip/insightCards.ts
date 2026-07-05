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
 *  - An optional cross-session baseline (`{median, n}` per metric, PR-3 §3a)
 *    renders a chip + "x.x× project median" position on each of the 5 cards;
 *    n<3 degrades to a "표본 부족" notice instead (see `compareToBaseline`).
 */
import type {
  SessionUsageDto,
  VerificationRunDto,
  SignalDto,
  TurnRollupDto,
  UsageBaselineDto,
  BaselineStat,
} from '../../../api/types';
import { formatPct, formatTokens, formatUsd } from '../../../lib/format';
import type { TFunction } from '../../../i18n';
import type { Provenance } from './provenance';

/** PR-3 §3a — one metric's cross-session median + sample size. `median` is
 *  null when the scope has no sessions with a value for this metric. */
export interface BaselineMedian {
  median: number | null;
  n: number;
}

/** Optional cross-session baseline (slice 6 + PR-3 §3a). Absent fields → no
 *  comparison for that card. */
export interface InsightBaseline {
  /** "project" | "store" — 백엔드가 내려주는 스코프 관측 사실(감사 F1,
   *  2026-07-05). store 폴백(프로젝트 미상)이면 위치 문구가 전체 세션 기준을
   *  명시해야 한다 — '프로젝트 중앙값' 하드코딩은 §0 표본 정직성 위반. */
  scope?: string;
  cache_hit_ratio?: BaselineMedian;
  billed_tokens?: BaselineMedian;
  verification_pass_rate?: BaselineMedian;
  tool_failure_count?: BaselineMedian;
  estimated_cost_usd?: BaselineMedian;
}

/** PR-3 §3a — a card's comparison against its cross-session median: the
 *  DeltaChip input (already-computed delta), a "x.x× median" position string,
 *  the sample size, and a lowSample flag (n<3 → chip/position dropped in
 *  favor of a "표본 부족" notice; 표기 원칙: 판정 문장 금지). */
export interface BaselineComparison {
  /** DeltaChip 입력 (v는 이미 계산된 delta 수치) */
  chip: { v: number; unit: string; betterUp: boolean };
  /** "프로젝트 중앙값의 x.x×" 본문 — median>0일 때만 */
  position?: string;
  n: number;
  /** n<3 — 칩·위치 대신 "표본 부족 (n N)"만 표기 */
  lowSample: boolean;
}

/** PR-3 §3a — maps the 7-metric baseline DTO down to the 5 metrics the insight
 *  cards compare against (median + n only; p25/p75 are not used here). */
export function toInsightBaseline(dto: UsageBaselineDto): InsightBaseline {
  const m = (s: BaselineStat): BaselineMedian => ({ median: s.median, n: s.n });
  return {
    scope: dto.scope,
    cache_hit_ratio: m(dto.cache_hit_ratio),
    billed_tokens: m(dto.billed_tokens),
    verification_pass_rate: m(dto.verification_pass_rate),
    tool_failure_count: m(dto.tool_failure_count),
    estimated_cost_usd: m(dto.estimated_cost_usd),
  };
}

/** PR-3 §3a — $/1M billed tokens, a blended unit-rate subtitle for the cost
 *  card (mixed models in a session have no single "rate" otherwise). Null
 *  when there are no billed tokens to divide by (미측정 ≠ 0). */
export function blendedRatePerMTok(costUsd: number, billedTokens: number): number | null {
  return billedTokens > 0 ? (costUsd / billedTokens) * 1_000_000 : null;
}

/** PR-3 §3a — builds a card's baseline comparison. Returns undefined when
 *  there is no baseline for this metric or its median is unmeasured (null).
 *  n<3 → lowSample (chip/position dropped; 표본 부족 표기만). */
function compareToBaseline(
  value: number,
  base: BaselineMedian | undefined,
  chip: { v: (value: number, median: number) => number; unit: string; betterUp: boolean },
  t: TFunction,
  scope?: string,
): BaselineComparison | undefined {
  if (!base || base.median === null) return undefined;
  if (base.n < 3) {
    return { chip: { v: 0, unit: chip.unit, betterUp: chip.betterUp }, n: base.n, lowSample: true };
  }
  const positionKey =
    scope === 'store' ? 'insight.baselinePositionStoreN' : 'insight.baselinePositionN';
  const position =
    base.median > 0
      ? t(positionKey, { x: (value / base.median).toFixed(1), n: base.n })
      : undefined;
  return {
    chip: { v: chip.v(value, base.median), unit: chip.unit, betterUp: chip.betterUp },
    position,
    n: base.n,
    lowSample: false,
  };
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
  /** Localized card title shown in the strip. */
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
  /** Optional cross-session comparison (slice 6 + PR-3 §3a); undefined when
   *  no baseline is supplied for this metric or the session value itself is
   *  unmeasured. */
  baseline?: BaselineComparison;
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

function contextCard(inputs: InsightInputs, t: TFunction): InsightCardModel {
  const tip = t('insight.context.tip');
  if (!inputs.usage) {
    return {
      id: 'context', title: t('insight.context.title'), value: '—',
      detail: t('insight.recollectUsage'), provenance: 'uncollected', tooltip: tip,
    };
  }
  const u = inputs.usage;
  const ratio = cacheHitRatio(u);
  const card: InsightCardModel = {
    id: 'context', title: t('insight.context.title'),
    value: formatPct(ratio),
    detail: t('insight.context.detailCacheRead', formatTokens(u.cache_read_input_tokens)),
    provenance: 'measured', tooltip: tip,
    drill: {
      lines: [
        t('insight.context.drillHitRate', formatPct(ratio)),
        t('insight.context.drillCacheReadFree', formatTokens(u.cache_read_input_tokens)),
        t('insight.context.drillCacheCreation', formatTokens(u.cache_creation_input_tokens)),
        t('insight.context.drillUserTurns', u.user_turns),
      ],
    },
  };
  if (typeof ratio === 'number') {
    card.baseline = compareToBaseline(
      ratio,
      inputs.baseline?.cache_hit_ratio,
      { v: (s, m) => (s - m) * 100, unit: '%p', betterUp: true },
      t,
      inputs.baseline?.scope,
    );
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

function tokensCard(inputs: InsightInputs, t: TFunction): InsightCardModel {
  const tip = t('insight.tokens.tip');
  if (!inputs.usage) {
    return {
      id: 'tokens', title: t('insight.tokens.title'), value: '—',
      detail: t('insight.recollectUsage'), provenance: 'uncollected', tooltip: tip,
    };
  }
  const u = inputs.usage;
  const card: InsightCardModel = {
    id: 'tokens', title: t('insight.tokens.title'),
    value: t('insight.tokens.valueBilled', formatTokens(u.billed_tokens)),
    detail: t('insight.tokens.detailCacheReadFree', formatTokens(u.cache_read_input_tokens)),
    provenance: 'measured', tooltip: tip,
    drill: {
      lines: [
        `input ${formatTokens(u.input_tokens)}`,
        `cache_creation ${formatTokens(u.cache_creation_input_tokens)}`,
        `output ${formatTokens(u.output_tokens)}`,
        ...u.by_model.map((m) =>
          t('insight.tokens.drillByModel', {
            model: m.model,
            events: m.assistant_events,
            out: formatTokens(m.output_tokens),
          }),
        ),
      ],
    },
  };
  card.baseline = compareToBaseline(
    u.billed_tokens,
    inputs.baseline?.billed_tokens,
    { v: (s, m) => (m > 0 ? (s / m - 1) * 100 : 0), unit: '%', betterUp: false },
    t,
    inputs.baseline?.scope,
  );
  if (inputs.turns) {
    const spark = perTurnBilled(inputs.turns);
    if (spark) {
      card.sparkline = spark;
      card.sparklineTint = 'blue';
    }
  }
  return card;
}

function verificationCard(inputs: InsightInputs, t: TFunction): InsightCardModel {
  const tip = t('insight.verification.tip');
  const runs = inputs.verificationRuns;
  if (!runs || runs.length === 0) {
    return {
      id: 'verification', title: t('insight.verification.title'), value: '—',
      detail: runs ? t('insight.verification.noGuards') : t('insight.loading'),
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
  if (unknown > 0) detailParts.push(t('insight.verification.unmeasured', unknown));
  const card: InsightCardModel = {
    id: 'verification', title: t('insight.verification.title'),
    value:
      measured > 0
        ? t('insight.verification.valuePassed', { total: runs.length, passed, measured })
        : t('insight.verification.valueNoMeasure', runs.length),
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
          `${r.command_kind} → ${r.status}${r.status_provenance === 'estimated' ? t('insight.verification.estimatedSuffix') : ''}`,
      ),
      byKind,
    },
  };
  if (measured > 0) {
    card.baseline = compareToBaseline(
      passed / measured,
      inputs.baseline?.verification_pass_rate,
      { v: (s, m) => (s - m) * 100, unit: '%p', betterUp: true },
      t,
      inputs.baseline?.scope,
    );
  }
  return card;
}

function toolFailureCard(inputs: InsightInputs, t: TFunction): InsightCardModel {
  const tip = t('insight.toolFailure.tip');
  const sigs = inputs.signals;
  if (!sigs) {
    return { id: 'tool_failure', title: t('insight.toolFailure.title'), value: '—', detail: t('insight.loading'), provenance: 'uncollected', tooltip: tip };
  }
  const failures = sigs.filter((s) => s.detector === 'tool_failure');
  const card: InsightCardModel = {
    id: 'tool_failure', title: t('insight.toolFailure.title'),
    value: `${failures.length}`,
    detail: failures.length === 0 ? t('insight.toolFailure.none') : t('insight.toolFailure.expand'),
    provenance: 'measured', tooltip: tip,
    drill: { lines: failures.map((s) => `${s.subkind ?? s.detector} · ${s.summary}`) },
  };
  card.baseline = compareToBaseline(
    failures.length,
    inputs.baseline?.tool_failure_count,
    { v: (s, m) => s - m, unit: '', betterUp: false },
    t,
    inputs.baseline?.scope,
  );
  return card;
}

function costCard(inputs: InsightInputs, t: TFunction): InsightCardModel {
  if (!inputs.usage) {
    return {
      id: 'cost', title: t('insight.cost.title'), value: '—',
      detail: t('insight.recollectUsage'), provenance: 'uncollected',
      tooltip: t('insight.cost.tip'),
    };
  }
  const u = inputs.usage;
  // §2.3 — 정적 추정 근거 + 관측 모델 단가 줄 + 가격표 기준일을 동적 조립.
  const tipLines: string[] = [t('insight.cost.tip')];
  for (const m of u.by_model) {
    tipLines.push(
      m.rates
        ? t('insight.cost.tipRateLine', {
            model: m.model,
            input: String(m.rates.input_per_mtok),
            output: String(m.rates.output_per_mtok),
            cacheRead: String(m.rates.cache_read_per_mtok),
            cacheWrite: String(m.rates.cache_creation_per_mtok),
          })
        : t('insight.cost.tipRateLineUnpriced', m.model),
    );
  }
  const versionDate = u.pricing_version.split('@')[1];
  if (versionDate) tipLines.push(t('insight.cost.tipPricingDate', versionDate));

  const unpriced = u.models_without_pricing.length > 0;
  const estimateDetail = unpriced
    ? t('insight.cost.detailEstimateUnpriced', u.models_without_pricing.length)
    : t('insight.cost.detailEstimate');
  // PR-3 §3a — a mixed-model session has no single "the rate", so subtitle
  // with a blended $/1M billed-tokens rate instead (미측정 ≠ 0 → '—' when
  // there are no billed tokens to divide by).
  const rate = blendedRatePerMTok(u.estimated_cost_usd, u.billed_tokens);
  const rateText =
    rate !== null
      ? t('insight.cost.detailUnitRate', rate.toFixed(2))
      : t('insight.cost.detailUnitRateNone');
  const card: InsightCardModel = {
    id: 'cost', title: t('insight.cost.title'),
    value: formatUsd(u.estimated_cost_usd),
    detail: `${estimateDetail} · ${rateText}`,
    provenance: 'estimated', tooltip: tipLines.join('\n'),
    drill: {
      lines: u.by_model.map(
        (m) => `${m.model}: ${m.priced ? formatUsd(m.estimated_cost_usd) : t('insight.cost.noPricing')}`,
      ),
    },
  };
  card.baseline = compareToBaseline(
    u.estimated_cost_usd,
    inputs.baseline?.estimated_cost_usd,
    { v: (s, m) => (m > 0 ? (s / m - 1) * 100 : 0), unit: '%', betterUp: false },
    t,
    inputs.baseline?.scope,
  );
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

export function buildInsightCards(inputs: InsightInputs, t: TFunction): InsightCardModel[] {
  return [
    contextCard(inputs, t),
    tokensCard(inputs, t),
    verificationCard(inputs, t),
    toolFailureCard(inputs, t),
    costCard(inputs, t),
  ];
}
