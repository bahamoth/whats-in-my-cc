/**
 * TanStack Query hooks for the read-only viewer.
 *
 * Each hook must:
 *  - fetch via the existing api/client helpers
 *  - cache by a stable key shape so downstream code can read it without re-fetching
 *  - filter out signals with empty `evidence_refs` (Evidence-linked invariant
 *    from CLAUDE.md — "Finding/RootCauseHypothesis/QualitySummary는
 *    evidence_refs 없이 만들지 않는다")
 */
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { createQueryClient } from '../queryClient';
import {
  useSessionDetailQuery,
  useSignalsQuery,
  useUsageBaselineQuery,
  sessionKeys,
  usageKeys,
} from '../queries';

const ENVELOPE = (data: unknown) => ({ meta: { generated_at: '2026-05-29T00:00:00Z' }, data });

function mockOk(payload: unknown) {
  return {
    ok: true,
    status: 200,
    statusText: 'OK',
    json: async () => payload,
  } as Response;
}

let fetchSpy: ReturnType<typeof vi.fn>;

function wrap(qc: QueryClient) {
  return ({ children }: { children: React.ReactNode }) => (
    <QueryClientProvider client={qc}>{children}</QueryClientProvider>
  );
}

beforeEach(() => {
  fetchSpy = vi.fn();
  vi.stubGlobal('fetch', fetchSpy);
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('sessionKeys', () => {
  it('produces stable, hierarchical query keys', () => {
    expect(sessionKeys.detail('S1')).toEqual(['session', 'S1', 'detail']);
    expect(sessionKeys.signals('S1')).toEqual(['session', 'S1', 'signals']);
  });
});

describe('useSessionDetailQuery', () => {
  it('caches the session summary under sessionKeys.detail(id)', async () => {
    const payload = { session_id: 'S1', summary: { event_count: 3, by_kind: {}, first_observed_at: '', last_observed_at: '' } };
    fetchSpy.mockResolvedValue(mockOk(ENVELOPE(payload)));
    const qc = createQueryClient();
    const { result } = renderHook(() => useSessionDetailQuery('S1'), { wrapper: wrap(qc) });
    await waitFor(() => expect(result.current.data).toEqual(payload));
    expect(qc.getQueryData(sessionKeys.detail('S1'))).toEqual(payload);
  });
});

describe('useSignalsQuery', () => {
  it('drops signals with empty evidence_refs (evidence-linked invariant)', async () => {
    const payload = [
      {
        signal_id: 'good',
        schema_version: '1',
        session_id: 'S1',
        detector: 'tool_failure',
        subkind: null,
        summary: 'ok',
        evidence_refs: ['01KSQKD5CT8BHH1DAS4YNKJBVB'],
        facts: {},
        provenance: {},
        created_at: '',
      },
      {
        signal_id: 'bad',
        schema_version: '1',
        session_id: 'S1',
        detector: 'context_bloat',
        subkind: null,
        summary: 'no evidence',
        evidence_refs: [],
        facts: {},
        provenance: {},
        created_at: '',
      },
    ];
    fetchSpy.mockResolvedValue(mockOk({ data: payload }));
    const qc = createQueryClient();
    const { result } = renderHook(() => useSignalsQuery('S1'), { wrapper: wrap(qc) });
    await waitFor(() => expect(result.current.data?.length).toBe(1));
    expect(result.current.data?.[0]?.signal_id).toBe('good');
  });
});

describe('useUsageBaselineQuery', () => {
  it('caches the store-wide baseline under [...usageKeys.baseline(), "store"] when no sessionId is given', async () => {
    const payload = {
      session_count: 2,
      cache_hit_ratio: { p25: 0.0, median: 0.45, p75: 0.9 },
      billed_tokens: { p25: 200, median: 300, p75: 400 },
      assistant_events: { p25: 1, median: 1, p75: 1 },
      output_tokens: { p25: 100, median: 200, p75: 300 },
    };
    fetchSpy.mockResolvedValue(mockOk(ENVELOPE(payload)));
    const qc = createQueryClient();
    const { result } = renderHook(() => useUsageBaselineQuery(), { wrapper: wrap(qc) });
    await waitFor(() => expect(result.current.data).toEqual(payload));
    expect(qc.getQueryData([...usageKeys.baseline(), 'store'])).toEqual(payload);
  });

  it('PR-3 §3a — scopes by sessionId: distinct cache key + session_id query param', async () => {
    const payload = {
      session_count: 2,
      cache_hit_ratio: { p25: 0.0, median: 0.45, p75: 0.9 },
      billed_tokens: { p25: 200, median: 300, p75: 400 },
      assistant_events: { p25: 1, median: 1, p75: 1 },
      output_tokens: { p25: 100, median: 200, p75: 300 },
    };
    fetchSpy.mockResolvedValue(mockOk(ENVELOPE(payload)));
    const qc = createQueryClient();
    const { result } = renderHook(() => useUsageBaselineQuery('s1'), { wrapper: wrap(qc) });
    await waitFor(() => expect(result.current.data).toEqual(payload));
    expect(fetchSpy).toHaveBeenCalledWith('/v1/usage/baseline?session_id=s1', expect.any(Object));
    expect(qc.getQueryData([...usageKeys.baseline(), 's1'])).toEqual(payload);
  });
});
