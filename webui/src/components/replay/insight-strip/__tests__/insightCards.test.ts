import { describe, expect, it } from 'vitest';
import { buildInsightCards, type InsightInputs } from '../insightCards';
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

describe('buildInsightCards — baseline delta (slice 6, optional)', () => {
  it('attaches a vs-median delta to the context card when a baseline is supplied', () => {
    const c = byId({ ...EMPTY, usage, baseline: { cache_hit_ratio: 0.9 } }).get('context')!;
    expect(c.baselineDelta).toBeDefined();
  });
  it('omits the delta gracefully when no baseline is supplied', () => {
    const c = byId({ ...EMPTY, usage }).get('context')!;
    expect(c.baselineDelta).toBeUndefined();
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

  it('attaches a billed-tokens vs-median delta to the tokens card', () => {
    const c = byId({
      ...EMPTY, usage,
      baseline: { cache_hit_ratio: 0.9, billed_tokens: 2_700_000 },
    }).get('tokens')!;
    // usage.billed_tokens = 5.4M vs median 2.7M → +100%
    expect(c.baselineDelta).toBe('+100% vs 중앙값');
  });
});
