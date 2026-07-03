/**
 * Pull API endpoint helpers. We assert that each helper hits the exact URL
 * the backend exposes and returns the envelope's `data` field unwrapped, so
 * callers never accidentally forget the `.data` indirection.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  getDiffHunks,
  getSessionMetrics,
  getSignals,
  getSessionUsage,
  getUsageBaseline,
  getVerificationRuns,
} from '../client';
import type {
  SessionMetricsDto,
  SessionUsageDto,
  UsageBaselineDto,
} from '../types';

const ENVELOPE = (data: unknown) => ({ meta: { generated_at: '2026-05-29T00:00:00Z' }, data });

function mockJson(payload: unknown) {
  return vi.fn(async () => ({
    ok: true,
    status: 200,
    statusText: 'OK',
    json: async () => payload,
  })) as unknown as typeof fetch;
}

let fetchSpy: ReturnType<typeof vi.fn>;

beforeEach(() => {
  fetchSpy = vi.fn();
  vi.stubGlobal('fetch', fetchSpy);
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('getDiffHunks', () => {
  it('hits GET /v1/sessions/:id/diff-hunks and unwraps `data`', async () => {
    const expected = [{ diff_hunk_id: 'dh1', file_path: 'a.rs' }];
    fetchSpy.mockImplementation(mockJson(ENVELOPE({ hunks: expected })));
    const out = await getDiffHunks('SES-1');
    expect(fetchSpy).toHaveBeenCalledWith('/v1/sessions/SES-1/diff-hunks', expect.any(Object));
    expect(out).toEqual(expected);
  });
});

describe('getSignals', () => {
  it('hits GET /v1/sessions/:id/signals and unwraps `data`', async () => {
    const expected = [{ signal_id: 'sig1', detector: 'tool_failure', evidence_refs: ['ev1'], facts: {}, summary: 's' }];
    fetchSpy.mockImplementation(mockJson({ data: expected }));
    const out = await getSignals('SES-1');
    expect(fetchSpy).toHaveBeenCalledWith('/v1/sessions/SES-1/signals', expect.any(Object));
    expect(out).toEqual(expected);
  });
});

describe('getSessionUsage', () => {
  it('getSessionUsage unwraps the usage envelope', async () => {
    const expected: SessionUsageDto = {
      session_id: 's1',
      assistant_events: 3,
      user_turns: 2,
      input_tokens: 10,
      cache_creation_input_tokens: 20,
      cache_read_input_tokens: 900,
      output_tokens: 30,
      billed_tokens: 60,
      estimated_cost_usd: 0.0123,
      cost_basis: 'estimate_public_pricing',
      pricing_version: 'pricing_estimate@v1',
      models_without_pricing: [],
      by_model: [
        {
          model: 'claude-opus-4-7',
          assistant_events: 3,
          input_tokens: 10,
          cache_creation_input_tokens: 20,
          cache_read_input_tokens: 900,
          output_tokens: 30,
          estimated_cost_usd: 0.0123,
          priced: true,
        },
      ],
    };
    fetchSpy.mockImplementation(mockJson(ENVELOPE(expected)));
    const out = await getSessionUsage('s1');
    expect(out).toEqual(expected);
  });
});

describe('getUsageBaseline', () => {
  it('hits GET /v1/usage/baseline and unwraps the envelope `data`', async () => {
    const expected: UsageBaselineDto = {
      session_count: 2,
      cache_hit_ratio: { p25: 0.0, median: 0.45, p75: 0.9 },
      billed_tokens: { p25: 200, median: 300, p75: 400 },
      assistant_events: { p25: 1, median: 1, p75: 1 },
      output_tokens: { p25: 100, median: 200, p75: 300 },
    };
    fetchSpy.mockImplementation(mockJson(ENVELOPE(expected)));
    const out = await getUsageBaseline();
    expect(fetchSpy).toHaveBeenCalledWith('/v1/usage/baseline', expect.any(Object));
    expect(out).toEqual(expected);
  });
});

describe('getSessionMetrics', () => {
  it('hits GET /v1/sessions/:id/metrics and unwraps `data`', async () => {
    const expected: SessionMetricsDto = {
      session_id: 's1',
      tool_call_total: 10,
      tool_failure_count: 2,
      verification_total: 4,
      verification_passed: 3,
      verification_failed: 1,
      verification_unknown: 0,
      verification_not_executed: 0,
      context_bloat_count: 1,
  tool_user_rejected: 0,
  tool_policy_denied: 0,
  tool_cancelled: 0,
  tool_backgrounded: 0,
      turn_duration_ms_total: 139516,
      turn_duration_count: 1,
      api_error_count: 1,
      api_rate_limit_count: 0,
      input_tokens: 0,
      output_tokens: 0,
      cache_read_input_tokens: 0,
      cache_creation_input_tokens: 0,
      compact_boundary_count: 1,
      tool_result_truncated_count: 1,
      user_interruption_count: 2,
      detector_firing: { tool_failure: 2, context_bloat: 1 },
    };
    fetchSpy.mockImplementation(mockJson(ENVELOPE(expected)));
    const out = await getSessionMetrics('s1');
    expect(fetchSpy).toHaveBeenCalledWith('/v1/sessions/s1/metrics', expect.any(Object));
    expect(out).toEqual(expected);
  });
});

describe('getVerificationRuns', () => {
  it('hits GET /v1/sessions/:id/verification-runs and unwraps `data` (single-wrapped array)', async () => {
    const expected = [{ verification_run_id: 'vr1', status: 'passed', covered_diff_hunk_ids: ['dh1'] }];
    // The backend returns { meta, data: [...] } — `data` IS the array, NOT a
    // further { data: [...] } wrapper. Verified against the running server:
    // `GET /v1/sessions/:id/verification-runs` → top keys [meta, data], data is a list.
    fetchSpy.mockImplementation(mockJson(ENVELOPE(expected)));
    const out = await getVerificationRuns('SES-4');
    expect(fetchSpy).toHaveBeenCalledWith(
      '/v1/sessions/SES-4/verification-runs',
      expect.any(Object),
    );
    expect(out).toEqual(expected);
  });
});
