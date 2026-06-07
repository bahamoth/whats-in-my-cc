import { describe, expect, it } from 'vitest';
import { buildInsightCards, type InsightInputs } from '../insightCards';
import type {
  SessionUsageDto,
  VerificationRunDto,
  SignalDto,
} from '../../../../api/types';

// Real SessionUsageDto shape (slice-5 added estimated_cost_usd / cost_basis /
// pricing_version / models_without_pricing; ModelUsageDto carries the full
// token + price breakdown). See webui/src/api/types.ts.
const usage: SessionUsageDto = {
  session_id: 's1',
  turns: 5,
  input_tokens: 200_000,
  cache_creation_input_tokens: 3_900_000,
  cache_read_input_tokens: 199_500_000,
  output_tokens: 1_300_000,
  billed_tokens: 5_400_000,
  cache_hit_ratio: 0.98,
  estimated_cost_usd: 102.5,
  cost_basis: 'estimate_public_pricing',
  pricing_version: 'v1',
  models_without_pricing: [],
  by_model: [
    {
      model: 'claude-opus-4-8',
      turns: 3,
      input_tokens: 150_000,
      cache_creation_input_tokens: 3_000_000,
      cache_read_input_tokens: 150_000_000,
      output_tokens: 1_000_000,
      estimated_cost_usd: 90,
      priced: true,
    },
    {
      model: 'claude-haiku-4-5-20251001',
      turns: 2,
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
  return new Map(buildInsightCards(inputs).map((c) => [c.id, c]));
}

describe('buildInsightCards — card set', () => {
  it('emits exactly the five redesigned cards in order, dropping the old tiles', () => {
    const ids = buildInsightCards(EMPTY).map((c) => c.id);
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
    // 5 guards, 3 passed
    expect(c.value).toBe('가드 5 · 통과 3');
    // all known_tool + exit basis → measured (slice-2 fields present)
    expect(c.provenance).toBe('measured');
    // by-kind breakdown is carried for the drill
    expect(c.drill?.byKind).toEqual({ test: 2, build: 1, lint: 1, format: 1 });
  });

  it('degrades to 혼합 (mixed) when any run was keyword-detected or piped (slice-2 basis)', () => {
    const runs = [
      vr('test_suite_rust', 'passed', 'known_tool', 'exit'),
      vr('test_suite_py', 'passed', 'test_keyword', 'piped'),
    ];
    const c = byId({ ...EMPTY, verificationRuns: runs }).get('verification')!;
    expect(c.provenance).toBe('mixed');
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
