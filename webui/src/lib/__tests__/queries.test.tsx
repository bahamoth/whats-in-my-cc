/**
 * PR-2 RED — TanStack Query hooks for the read-only viewer.
 *
 * Each hook must:
 *  - fetch via the existing api/client helpers
 *  - cache by a stable key shape so PR-3+ can read it without re-fetching
 *  - filter out findings with empty `evidence_refs` (Evidence-linked invariant
 *    from CLAUDE.md — "Finding/RootCauseHypothesis/QualitySummary는
 *    evidence_refs 없이 만들지 않는다")
 *
 * See plan §10.1 PR-2 (revised).
 */
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { createQueryClient } from '../queryClient';
import {
  useSessionDetailQuery,
  useSessionGraphQuery,
  useEpisodesQuery,
  useFindingsQuery,
  sessionKeys,
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
    expect(sessionKeys.graph('S1')).toEqual(['session', 'S1', 'graph']);
    expect(sessionKeys.episodes('S1')).toEqual(['session', 'S1', 'episodes']);
    expect(sessionKeys.findings('S1')).toEqual(['session', 'S1', 'findings']);
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

describe('useSessionGraphQuery', () => {
  it('caches the graph payload under sessionKeys.graph(id)', async () => {
    const payload = { nodes: [], edges: [] };
    fetchSpy.mockResolvedValue(mockOk(ENVELOPE(payload)));
    const qc = createQueryClient();
    const { result } = renderHook(() => useSessionGraphQuery('S1'), { wrapper: wrap(qc) });
    await waitFor(() => expect(result.current.data).toEqual(payload));
    expect(qc.getQueryData(sessionKeys.graph('S1'))).toEqual(payload);
  });
});

describe('useEpisodesQuery', () => {
  it('caches the episodes array under sessionKeys.episodes(id)', async () => {
    const payload = [{ episode_id: 'ep1', phase: 'action', confidence: 0.7 }];
    // Backend returns the array directly under `data` (no `meta` envelope).
    fetchSpy.mockResolvedValue(mockOk({ data: payload }));
    const qc = createQueryClient();
    const { result } = renderHook(() => useEpisodesQuery('S1'), { wrapper: wrap(qc) });
    await waitFor(() => expect(result.current.data).toEqual(payload));
  });
});

describe('useFindingsQuery', () => {
  it('drops findings with empty evidence_refs (evidence-linked invariant)', async () => {
    const payload = [
      {
        finding_id: 'good',
        category: 'risky_action',
        severity: 'high',
        confidence: 0.9,
        // Slice-14 deterministic extractors emit bare ULID strings here.
        evidence_refs: ['01KSQKD5CT8BHH1DAS4YNKJBVB'],
        evidence_projection: {},
        provenance: {},
        summary: 'ok',
        status: 'active',
      },
      {
        finding_id: 'bad',
        category: 'context_bloat',
        severity: 'low',
        confidence: 0.5,
        evidence_refs: [],
        evidence_projection: {},
        provenance: {},
        summary: 'no evidence',
        status: 'active',
      },
    ];
    fetchSpy.mockResolvedValue(mockOk({ data: payload }));
    const qc = createQueryClient();
    const { result } = renderHook(() => useFindingsQuery('S1'), { wrapper: wrap(qc) });
    await waitFor(() => expect(result.current.data?.length).toBe(1));
    expect(result.current.data?.[0]?.finding_id).toBe('good');
  });
});
