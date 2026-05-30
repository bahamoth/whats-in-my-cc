/**
 * PR-2 RED — new Pull API endpoint helpers. We assert that each helper hits
 * the exact URL the backend exposes (per `docs/04_api_mcp_spec.html`) and
 * returns the envelope's `data` field unwrapped, so callers never accidentally
 * forget the `.data` indirection. See plan §10.1 PR-2 (revised).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  getDiffHunks,
  getEpisodes,
  getFindings,
  getFindingEvidence,
  getSessionUsage,
  getToolFailureSummary,
  getVerificationRuns,
} from '../client';

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

describe('getEpisodes', () => {
  it('hits GET /v1/sessions/:id/episodes (backend has no meta envelope here)', async () => {
    const expected = [{ episode_id: 'ep1', phase: 'action', confidence: 0.8 }];
    // Backend response shape: { data: [...] } — no `meta` field.
    fetchSpy.mockImplementation(mockJson({ data: expected }));
    const out = await getEpisodes('SES-2');
    expect(fetchSpy).toHaveBeenCalledWith('/v1/sessions/SES-2/episodes', expect.any(Object));
    expect(out).toEqual(expected);
  });
});

describe('getFindings', () => {
  it('hits GET /v1/sessions/:id/findings (no meta envelope; evidence_refs may be ULID strings)', async () => {
    const expected = [
      {
        finding_id: 'f1',
        category: 'risky_action',
        severity: 'high',
        confidence: 0.9,
        // Slice-14 deterministic extractors emit bare ULIDs here, not objects.
        evidence_refs: ['01KSQKD5CT8BHH1DAS4YNKJBVB'],
      },
    ];
    fetchSpy.mockImplementation(mockJson({ data: expected }));
    const out = await getFindings('SES-3');
    expect(fetchSpy).toHaveBeenCalledWith('/v1/sessions/SES-3/findings', expect.any(Object));
    expect(out).toEqual(expected);
  });
});

describe('getFindingEvidence', () => {
  it('hits GET /v1/findings/:id/evidence and unwraps the meta envelope `data`', async () => {
    const expected = { finding: { finding_id: 'f1' }, subgraph: { nodes: [], edges: [] }, raw_source_refs: [] };
    fetchSpy.mockImplementation(mockJson(ENVELOPE(expected)));
    const out = await getFindingEvidence('f1');
    expect(fetchSpy).toHaveBeenCalledWith('/v1/findings/f1/evidence', expect.any(Object));
    expect(out).toEqual(expected);
  });
});

describe('getSessionUsage', () => {
  it('getSessionUsage unwraps the usage envelope', async () => {
    const expected = {
      session_id: 's1',
      turns: 3,
      input_tokens: 10,
      cache_creation_input_tokens: 20,
      cache_read_input_tokens: 900,
      output_tokens: 30,
      billed_tokens: 60,
      cache_hit_ratio: 0.96,
      estimated_cost_usd: 0.0123,
      cost_basis: 'estimate_public_pricing',
      pricing_version: 'pricing_estimate@v1',
      models_without_pricing: [],
      by_model: [
        {
          model: 'claude-opus-4-7',
          turns: 3,
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

describe('getToolFailureSummary', () => {
  it('hits GET /v1/sessions/:id/tool-failures and unwraps `data`', async () => {
    const expected = {
      session_id: 's1', user_visible: 28, internal_retry: 1941,
      benign_nonzero_exit: 12, unclassified: 0, total: 1981, user_visible_findings: [],
    };
    fetchSpy.mockImplementation(mockJson(ENVELOPE(expected)));
    const out = await getToolFailureSummary('s1');
    expect(out).toEqual(expected);
    expect(fetchSpy).toHaveBeenCalledWith(
      expect.stringContaining('/v1/sessions/s1/tool-failures'),
      expect.any(Object),
    );
  });
});

describe('getVerificationRuns', () => {
  it('hits GET /v1/sessions/:id/verification-runs and unwraps `data` (single-wrapped array)', async () => {
    const expected = [{ verification_run_id: 'vr1', status: 'passed', covered_diff_hunk_ids: ['dh1'] }];
    // The backend returns { meta, data: [...] } — `data` IS the array, NOT a
    // further { data: [...] } wrapper. Verified against the running server:
    // `GET /v1/sessions/:id/verification-runs` → top keys [meta, data], data is a list.
    // The previous test mocked the double-wrapped shape, hiding a real bug where
    // the client over-unwrapped and returned undefined (broke KPI coverage).
    fetchSpy.mockImplementation(mockJson(ENVELOPE(expected)));
    const out = await getVerificationRuns('SES-4');
    expect(fetchSpy).toHaveBeenCalledWith(
      '/v1/sessions/SES-4/verification-runs',
      expect.any(Object),
    );
    expect(out).toEqual(expected);
  });
});
