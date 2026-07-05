import { describe, expect, it } from 'vitest';
import {
  buildInsightCards,
  blendedRatePerMTok,
  toInsightBaseline,
  type InsightInputs,
} from '../insightCards';
import { translate } from '../../../../i18n/t';
import { en } from '../../../../i18n/catalog/en';
import { ko } from '../../../../i18n/catalog/ko';
import type { TFunction } from '../../../../i18n';
import type {
  SessionUsageDto,
  VerificationRunDto,
  SignalDto,
} from '../../../../api/types';

// Assert against the Korean (source) strings: build a t bound to the ko
// catalog and inject it, exactly as the running app does via the provider.
const koT: TFunction = (key, arg) => translate(ko, en, key, arg);

// Real SessionUsageDto shape (slice-5 added estimated_cost_usd / cost_basis /
// pricing_version / models_without_pricing; ModelUsageDto carries the full
// token + price breakdown). See webui/src/api/types.ts.
const usage: SessionUsageDto = {
  session_id: 's1',
  assistant_events: 5,
  user_turns: 5,
  input_tokens: 200_000,
  cache_creation_input_tokens: 3_900_000,
  cache_read_input_tokens: 199_500_000,
  output_tokens: 1_300_000,
  billed_tokens: 5_400_000,
  estimated_cost_usd: 102.5,
  cost_basis: 'estimate_public_pricing',
  pricing_version: 'v1',
  models_without_pricing: [],
  by_model: [
    {
      model: 'claude-opus-4-8',
      assistant_events: 3,
      input_tokens: 150_000,
      cache_creation_input_tokens: 3_000_000,
      cache_read_input_tokens: 150_000_000,
      output_tokens: 1_000_000,
      estimated_cost_usd: 90,
      priced: true,
      rates: null,
    },
    {
      model: 'claude-haiku-4-5-20251001',
      assistant_events: 2,
      input_tokens: 50_000,
      cache_creation_input_tokens: 900_000,
      cache_read_input_tokens: 49_500_000,
      output_tokens: 300_000,
      estimated_cost_usd: 12.5,
      priced: true,
      rates: null,
    },
  ],
};

// Real VerificationRunDto shape (slice-2 added detection_basis / status_basis).
function vr(
  kind: string,
  status: string,
  detection_basis = 'known_tool',
  status_basis = 'exit',
  status_provenance: string | null = 'measured',
): VerificationRunDto {
  return {
    verification_run_id: `vr_${kind}_${status}`,
    schema_version: '1',
    session_id: 's1',
    source: 'transcript_bash',
    command: kind,
    command_kind: kind,
    trigger_event_id: 'e',
    trigger_tool_use_id: null,
    status,
    status_provenance,
    detection_basis,
    status_basis,
    started_at: '2026-05-30T00:00:00Z',
    ended_at: null,
    exit_code: null,
    failure_summary: null,
    covered_diff_hunk_ids: [],
  };
}

function signal(detector: string, subkind: string | null = null, summary = 's'): SignalDto {
  return {
    signal_id: `sig_${detector}_${subkind ?? 'none'}`,
    schema_version: '1',
    session_id: 's1',
    detector,
    subkind,
    summary,
    evidence_refs: ['ev1'],
    facts: {},
    provenance: {},
    created_at: '',
  };
}

const EMPTY: InsightInputs = {
  usage: undefined,
  verificationRuns: undefined,
  signals: undefined,
};

function byId(inputs: InsightInputs) {
  return new Map(buildInsightCards(inputs, koT).map((c) => [c.id, c]));
}

describe('buildInsightCards — card set', () => {
  it('emits exactly the five redesigned cards in order, dropping the old tiles', () => {
    const ids = buildInsightCards(EMPTY, koT).map((c) => c.id);
    expect(ids).toEqual(['context', 'tokens', 'verification', 'tool_failure', 'cost']);
    // The removed tiles never appear (spec §1/§5/§11 P1).
    expect(ids).not.toContain('outcome');
    expect(ids).not.toContain('risk');
    expect(ids).not.toContain('episodes');
    expect(ids).not.toContain('latency');
  });
});

describe('buildInsightCards — context efficiency (Q1/Q5)', () => {
  it('shows cache-hit % (측정) from usage', () => {
    const c = byId({ ...EMPTY, usage }).get('context')!;
    expect(c.value).toBe('98%');
    expect(c.provenance).toBe('measured');
  });

  it('falls back to 미수집·예정 when usage is absent', () => {
    const c = byId(EMPTY).get('context')!;
    expect(c.provenance).toBe('uncollected');
    expect(c.value).toBe('—');
  });
});

describe('buildInsightCards — tokens (Q2)', () => {
  it('shows billed and cache-read SEPARATELY, never summed (spec §3 Q2)', () => {
    const c = byId({ ...EMPTY, usage }).get('tokens')!;
    // billed 5.4M vs cache-read 199.5M shown as distinct facts
    expect(c.value).toBe('청구 5.4M');
    expect(c.detail).toContain('199.5M');
    expect(c.provenance).toBe('measured');
  });
  it('is 미수집·예정 with placeholder when usage absent', () => {
    const c = byId(EMPTY).get('tokens')!;
    expect(c.provenance).toBe('uncollected');
    expect(c.value).toBe('—');
  });
});

describe('buildInsightCards — verification guards (Q4)', () => {
  it('groups command_kind into test/build/lint/format with pass counts', () => {
    const runs = [
      vr('test_suite_rust', 'passed'),
      vr('test_suite_js', 'failed'),
      vr('build', 'passed'),
      vr('lint', 'passed'),
      vr('format_check', 'skipped'),
    ];
    const c = byId({ ...EMPTY, verificationRuns: runs }).get('verification')!;
    // 5 guards, 3 passed / 1 failed → measured 4 (the 'skipped' run is neither).
    expect(c.value).toBe('가드 5 · 통과 3/4');
    // all known_tool + exit basis → measured (slice-2 fields present)
    expect(c.provenance).toBe('measured');
    // by-kind breakdown is carried for the drill
    expect(c.drill?.byKind).toEqual({ test: 2, build: 1, lint: 1, format: 1 });
  });

  it('surfaces the measured denominator and 미측정 count in the headline (dogfooding 2026-06-11)', () => {
    // The headline must not read "가드 N · 통과 P" with N as the implicit
    // denominator — most guards can be unmeasured (piped, no exit). Pass rate is
    // over MEASURED runs (passed+failed); unmeasured count is surfaced.
    const runs = [
      vr('test_suite_rust', 'passed'),
      vr('test_suite_js', 'failed'),
      vr('build', 'unknown', 'known_tool', 'piped', 'unknown'),
      vr('lint', 'unknown', 'known_tool', 'piped', 'unknown'),
    ];
    const c = byId({ ...EMPTY, verificationRuns: runs }).get('verification')!;
    expect(c.value).toBe('가드 4 · 통과 1/2');
    expect(c.detail).toContain('미측정 2');
  });

  it('degrades to 혼합 (mixed) when any run was keyword-detected or piped (slice-2 basis)', () => {
    const runs = [
      vr('test_suite_rust', 'passed', 'known_tool', 'exit'),
      vr('test_suite_py', 'passed', 'test_keyword', 'piped'),
    ];
    const c = byId({ ...EMPTY, verificationRuns: runs }).get('verification')!;
    expect(c.provenance).toBe('mixed');
  });

  it('drill rows mark estimated-status runs with (추정) after the status', () => {
    // status_provenance = how the per-run STATUS was determined (0022). This is
    // a different axis from the card badge (measured/mixed from
    // detection_basis/status_basis): an output-text heuristic can yield
    // estimated even on a known_tool + exit run shape.
    const runs = [
      vr('test_suite_rust', 'passed', 'known_tool', 'exit', 'measured'),
      vr('build', 'failed', 'known_tool', 'exit', 'estimated'),
    ];
    const c = byId({ ...EMPTY, verificationRuns: runs }).get('verification')!;
    expect(c.drill?.lines).toEqual([
      'test_suite_rust → passed',
      'build → failed (추정)',
    ]);
  });

  it('drill rows stay unmarked for measured / unknown / null status_provenance (pre-0022 rows)', () => {
    const runs = [
      vr('test_suite_rust', 'passed', 'known_tool', 'exit', 'measured'),
      vr('lint', 'unknown', 'known_tool', 'piped', 'unknown'),
      vr('build', 'passed', 'known_tool', 'exit', null),
    ];
    const c = byId({ ...EMPTY, verificationRuns: runs }).get('verification')!;
    expect(c.drill?.lines).toEqual([
      'test_suite_rust → passed',
      'lint → unknown',
      'build → passed',
    ]);
  });

  it('is 미수집·예정 when there are no verification runs', () => {
    const c = byId({ ...EMPTY, verificationRuns: [] }).get('verification')!;
    expect(c.provenance).toBe('uncollected');
    expect(c.value).toBe('—');
  });
});

describe('buildInsightCards — tool_failure card (signal-based)', () => {
  it('counts signals with detector=tool_failure and badges measured', () => {
    const sigs = [
      signal('tool_failure', 'non_zero_exit', 'exit 1'),
      signal('context_bloat', null, 'context growing'), // not a tool failure
      signal('tool_failure', 'permission_denied', 'mkdir failed'),
    ];
    const c = byId({ ...EMPTY, signals: sigs }).get('tool_failure')!;
    expect(c.value).toBe('2');
    expect(c.provenance).toBe('measured');
  });

  it('is 미수집·예정 when signals are not loaded (undefined)', () => {
    const c = byId(EMPTY).get('tool_failure')!;
    expect(c.provenance).toBe('uncollected');
    expect(c.value).toBe('—');
  });

  it('shows 0 (측정) when signals loaded but none are tool_failure', () => {
    const c = byId({ ...EMPTY, signals: [signal('context_bloat')] }).get('tool_failure')!;
    expect(c.value).toBe('0');
    expect(c.provenance).toBe('measured');
  });

  it('drill lines show subkind · summary', () => {
    const c = byId({ ...EMPTY, signals: [signal('tool_failure', 'non_zero_exit', 'exit 1')] }).get('tool_failure')!;
    expect(c.drill?.lines).toEqual(['non_zero_exit · exit 1']);
  });

  it('drill lines fall back to detector when subkind is null', () => {
    const c = byId({ ...EMPTY, signals: [signal('tool_failure', null, 'something broke')] }).get('tool_failure')!;
    expect(c.drill?.lines).toEqual(['tool_failure · something broke']);
  });
});

describe('buildInsightCards — cost (Q2, 추정)', () => {
  it('uses the backend public-pricing estimate from usage, badged 추정', () => {
    const c = byId({ ...EMPTY, usage }).get('cost')!;
    expect(c.provenance).toBe('estimated');
    // value is a formatted dollar string, never "billing"
    expect(c.value).toMatch(/^\$/);
  });
  it('is 미수집·예정 when usage absent', () => {
    const c = byId(EMPTY).get('cost')!;
    expect(c.provenance).toBe('uncollected');
    expect(c.value).toBe('—');
  });
});

describe('buildInsightCards — baseline comparison (PR-3 §3a)', () => {
  const baseline = {
    cache_hit_ratio: { median: 0.5, n: 12 },
    billed_tokens: { median: 1_000_000, n: 12 },
    verification_pass_rate: { median: 0.8, n: 5 },
    tool_failure_count: { median: 2, n: 12 },
    estimated_cost_usd: { median: 1.0, n: 12 },
  };

  it('context 카드: pp 칩 + 위치 + n', () => {
    // usage fixture의 cache_hit_ratio가 0.75라면 chip.v = 25(pp), position 1.5×.
    const c = byId({ ...EMPTY, usage, baseline }).get('context')!;
    expect(c.baseline).toBeDefined();
    expect(c.baseline!.chip.unit).toBe('%p');
    expect(c.baseline!.chip.betterUp).toBe(true);
    expect(c.baseline!.n).toBe(12);
    expect(c.baseline!.lowSample).toBe(false);
    expect(c.baseline!.position).toContain('×');
  });

  it('n<3이면 lowSample=true로 강조를 해제한다', () => {
    const low = { ...baseline, billed_tokens: { median: 1_000_000, n: 2 } };
    const c = byId({ ...EMPTY, usage, baseline: low }).get('tokens')!;
    expect(c.baseline!.lowSample).toBe(true);
  });

  it('median이 null이면 baseline을 붙이지 않는다', () => {
    const none = { ...baseline, estimated_cost_usd: { median: null, n: 0 } };
    const c = byId({ ...EMPTY, usage, baseline: none }).get('cost')!;
    expect(c.baseline).toBeUndefined();
  });

  it('baseline이 없으면 종전처럼 생략한다', () => {
    const c = byId({ ...EMPTY, usage }).get('context')!;
    expect(c.baseline).toBeUndefined();
  });

  it('tool_failure 카드: count−median 칩(단위 없음, 상승=앰버 방향)', () => {
    const sigs = [signal('tool_failure'), signal('tool_failure'), signal('tool_failure')];
    const c = byId({ ...EMPTY, usage, signals: sigs, baseline }).get('tool_failure')!;
    expect(c.baseline!.chip.v).toBe(1); // 3 − 2
    expect(c.baseline!.chip.betterUp).toBe(false);
  });

  it('verification 카드: MEASURED 통과율 기준 pp 칩 + 위치 + n (Task 3 review forward-risk)', () => {
    // passed=1, failed=1 → measured=2, pass rate=0.5; base median=0.8,n=5.
    const runs = [vr('test_suite_rust', 'passed'), vr('test_suite_js', 'failed')];
    const c = byId({ ...EMPTY, verificationRuns: runs, baseline }).get('verification')!;
    expect(c.baseline).toBeDefined();
    expect(c.baseline!.chip.unit).toBe('%p');
    expect(c.baseline!.chip.betterUp).toBe(true);
    expect(c.baseline!.chip.v).toBeCloseTo(-30); // (0.5 − 0.8) × 100
    expect(c.baseline!.n).toBe(5);
    expect(c.baseline!.lowSample).toBe(false);
    expect(c.baseline!.position).toContain('×');
  });

  it('verification 카드: 측정된 run이 없으면(measured=0) baseline을 붙이지 않는다', () => {
    // status='skipped' is neither passed nor failed → measured stays 0.
    const runs = [vr('format_check', 'skipped')];
    const c = byId({ ...EMPTY, verificationRuns: runs, baseline }).get('verification')!;
    expect(c.baseline).toBeUndefined();
  });
});

describe('blendedRatePerMTok + 비용 카드 부제 (PR-3 §3a)', () => {
  it('비용/과금토큰 → $/1M', () => {
    expect(blendedRatePerMTok(2, 1_000_000)).toBe(2);
    expect(blendedRatePerMTok(1, 500_000)).toBe(2);
  });
  it('분모 0이면 null', () => {
    expect(blendedRatePerMTok(2, 0)).toBeNull();
  });
  it('비용 카드 detail에 블렌디드 단가가 병기된다', () => {
    const c = byId({ ...EMPTY, usage }).get('cost')!;
    expect(c.detail).toContain('블렌디드');
  });
  it('과금 토큰이 0이면 블렌디드 단가 대신 — 를 표기한다 (미측정 ≠ 0)', () => {
    const zeroBilled = { ...usage, billed_tokens: 0 };
    const c = byId({ ...EMPTY, usage: zeroBilled }).get('cost')!;
    expect(c.detail).toContain('블렌디드 —');
  });
});

describe('toInsightBaseline (PR-3 §3a)', () => {
  it('DTO 7지표 중 카드 5지표를 median+n으로 사상한다', () => {
    const dto = {
      session_count: 3, scope: 'project', project: '/p',
      cache_hit_ratio: { p25: 0.1, median: 0.5, p75: 0.9, n: 3 },
      billed_tokens: { p25: 1, median: 2, p75: 3, n: 3 },
      assistant_events: { p25: 1, median: 1, p75: 1, n: 3 },
      output_tokens: { p25: 1, median: 1, p75: 1, n: 3 },
      verification_pass_rate: { p25: null, median: null, p75: null, n: 0 },
      tool_failure_count: { p25: 0, median: 1, p75: 2, n: 3 },
      estimated_cost_usd: { p25: 0.5, median: 1, p75: 2, n: 3 },
    };
    const b = toInsightBaseline(dto);
    expect(b.cache_hit_ratio).toEqual({ median: 0.5, n: 3 });
    expect(b.verification_pass_rate).toEqual({ median: null, n: 0 });
  });
});

// S8 (UX 재설계) — intra-session sparklines from per-turn tokens + tokens baseline.
describe('buildInsightCards — S8 sparklines (per-turn tokens)', () => {
  const turns = [
    {
      turn_id: 't1', first_observed_at: '', last_observed_at: '',
      tool_call_total: 0, tool_histogram: {}, tag_histogram: {}, files_edited: [],
      tokens: { input_tokens: 100, cache_creation_input_tokens: 0, cache_read_input_tokens: 900, output_tokens: 50 },
    },
    {
      turn_id: 't2', first_observed_at: '', last_observed_at: '',
      tool_call_total: 0, tool_histogram: {}, tag_histogram: {}, files_edited: [],
      tokens: { input_tokens: 200, cache_creation_input_tokens: 0, cache_read_input_tokens: 600, output_tokens: 100 },
    },
  ];

  it('context card gets a per-turn cache-hit-ratio sparkline (blue tint)', () => {
    const c = byId({ ...EMPTY, usage, turns }).get('context')!;
    // cacheHit per turn: 900/(900+0+100)=0.9, 600/(600+0+200)=0.75
    expect(c.sparkline).toEqual([0.9, 0.75]);
    expect(c.sparklineTint).toBe('blue');
  });

  it('tokens card gets a per-turn billed sparkline', () => {
    const c = byId({ ...EMPTY, usage, turns }).get('tokens')!;
    // billed per turn = input + cache_creation + output: 150, 300
    expect(c.sparkline).toEqual([150, 300]);
  });

  it('cost card reuses the per-turn billed sparkline (cost ∝ billed tokens)', () => {
    const c = byId({ ...EMPTY, usage, turns }).get('cost')!;
    expect(c.sparkline).toEqual([150, 300]);
    expect(c.sparklineTint).toBe('blue');
  });

  it('omits sparklines gracefully when no turn tokens are present', () => {
    const c = byId({ ...EMPTY, usage }).get('context')!;
    expect(c.sparkline).toBeUndefined();
  });

  it('attaches a billed-tokens vs-median comparison to the tokens card', () => {
    const c = byId({
      ...EMPTY, usage,
      baseline: {
        cache_hit_ratio: { median: 0.9, n: 12 },
        billed_tokens: { median: 2_700_000, n: 12 },
      },
    }).get('tokens')!;
    // usage.billed_tokens = 5.4M vs median 2.7M → +100%
    expect(c.baseline!.chip.v).toBe(100);
    expect(c.baseline!.chip.unit).toBe('%');
  });
});

// §2.3 — cost tooltip becomes a dynamic assembly: static estimate-basis copy +
// one rate line per model OBSERVED IN THIS SESSION + a pricing-date line.
describe('costCard tooltip — 단가표 동적 조립 (§2.3)', () => {
  const usageWithRates: SessionUsageDto = {
    session_id: 's1',
    assistant_events: 2,
    user_turns: 1,
    input_tokens: 1000,
    cache_creation_input_tokens: 0,
    cache_read_input_tokens: 0,
    output_tokens: 500,
    billed_tokens: 1500,
    estimated_cost_usd: 0.04,
    cost_basis: 'estimate_public_pricing',
    pricing_version: 'pricing_estimate@2026-06-11',
    models_without_pricing: ['some-future-model-x'],
    by_model: [
      {
        model: 'claude-fable-5',
        assistant_events: 1,
        input_tokens: 1000,
        cache_creation_input_tokens: 0,
        cache_read_input_tokens: 0,
        output_tokens: 500,
        estimated_cost_usd: 0.035,
        priced: true,
        rates: {
          input_per_mtok: 10,
          cache_creation_per_mtok: 12.5,
          cache_read_per_mtok: 1,
          output_per_mtok: 50,
        },
      },
      {
        model: 'some-future-model-x',
        assistant_events: 1,
        input_tokens: 0,
        cache_creation_input_tokens: 0,
        cache_read_input_tokens: 0,
        output_tokens: 0,
        estimated_cost_usd: 0,
        priced: false,
        rates: null,
      },
    ],
  };

  it('관측 모델의 단가 줄 + 기준일 줄이 툴팁에 붙는다', () => {
    const cards = byId({ ...EMPTY, usage: usageWithRates });
    const cost = cards.get('cost')!;
    expect(cost.tooltip).toContain('`claude-fable-5`');
    expect(cost.tooltip).toContain('$10');
    expect(cost.tooltip).toContain('$50');
    expect(cost.tooltip).toContain('12.5');
    expect(cost.tooltip).toContain('2026-06-11');
  });

  it('미가격 모델은 가격표 없음 줄로만 표기되고 $0/$undefined 단가가 새지 않는다 (미측정≠0)', () => {
    const cards = byId({ ...EMPTY, usage: usageWithRates });
    const cost = cards.get('cost')!;
    expect(cost.tooltip).toContain('`some-future-model-x`');
    // unpriced 브랜치만 내는 문구 — rate-line로 fallthrough하면 이 문구가 사라진다.
    expect(cost.tooltip).toContain('가격표 없음');
    // 그 모델 줄에 $0/$undefined 단가가 렌더되면 안 됨 (미측정을 0으로 표기 금지).
    expect(cost.tooltip).not.toMatch(/some-future-model-x[^\n]*\$(0\b|undefined)/);
  });

  it('usage 미수집이면 기존 정적 툴팁 그대로', () => {
    const cards = byId(EMPTY);
    const cost = cards.get('cost')!;
    expect(cost.tooltip).not.toContain('기준');
  });

  // 머지-후 감사 #1(2026-07-05, MEDIUM): 정적 tip이 cache_read를 '비용에서
  // 제외'라 했으나 실제 추정(pricing.rs)은 cache-read 단가로 포함한다 — 같은
  // 툴팁의 동적 단가 줄(cache-read $N/1M)과 정면 모순. 오기 재발을 잠근다.
  it('정적 tip이 cache_read 비용 제외를 주장하지 않는다 (계산과 모순 금지)', () => {
    const cards = byId({ ...EMPTY, usage: usageWithRates });
    const cost = cards.get('cost')!;
    expect(cost.tooltip).not.toMatch(/비용에서 제외|excluded from cost/);
  });
});
