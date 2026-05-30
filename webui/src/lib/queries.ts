/**
 * PR-2 — read-only React Query hooks for every session-scoped resource the
 * UI needs. Keys are arranged hierarchically so PR-3+ can invalidate at any
 * level (the whole session subtree, or just `graph`).
 *
 * Evidence-linked invariant: `useFindingsQuery` drops findings whose
 * `evidence_refs` array is empty. Per CLAUDE.md "Finding/RootCauseHypothesis/
 * QualitySummary는 evidence_refs 없이 만들지 않는다."
 */
import { useQuery, type UseQueryOptions } from '@tanstack/react-query';
import {
  getSession,
  getGraph,
  getEpisodes,
  getFindings,
  getFindingEvidence,
  getSessionUsage,
  getVerificationRuns,
  getDiffHunks,
  getEventRaw,
} from '../api/client';
import type {
  SessionDetail,
  GraphPayload,
  EpisodeDto,
  FindingDto,
  SessionUsageDto,
  VerificationRunDto,
  DiffHunkDto,
  FindingEvidenceResponse,
  RawEventResponse,
} from '../api/types';

export const sessionKeys = {
  /** Root key for a session — invalidate this to wipe every nested cache
   *  (used on SSE `resync`). */
  session: (id: string) => ['session', id] as const,
  detail: (id: string) => ['session', id, 'detail'] as const,
  graph: (id: string) => ['session', id, 'graph'] as const,
  events: (id: string) => ['session', id, 'events'] as const,
  episodes: (id: string) => ['session', id, 'episodes'] as const,
  findings: (id: string) => ['session', id, 'findings'] as const,
  diffHunks: (id: string) => ['session', id, 'diff-hunks'] as const,
  verificationRuns: (id: string) => ['session', id, 'verification-runs'] as const,
  usage: (id: string) => ['session', id, 'usage'] as const,
  findingEvidence: (findingId: string) => ['finding', findingId, 'evidence'] as const,
};

type QOpts<T> = Omit<UseQueryOptions<T, Error, T>, 'queryKey' | 'queryFn'>;

export function useSessionDetailQuery(id: string, opts?: QOpts<SessionDetail>) {
  return useQuery<SessionDetail>({
    queryKey: sessionKeys.detail(id),
    queryFn: () => getSession(id),
    enabled: !!id,
    ...opts,
  });
}

export function useSessionGraphQuery(id: string, opts?: QOpts<GraphPayload>) {
  return useQuery<GraphPayload>({
    queryKey: sessionKeys.graph(id),
    queryFn: () => getGraph(id),
    enabled: !!id,
    ...opts,
  });
}

export function useEpisodesQuery(id: string, opts?: QOpts<EpisodeDto[]>) {
  return useQuery<EpisodeDto[]>({
    queryKey: sessionKeys.episodes(id),
    queryFn: () => getEpisodes(id),
    enabled: !!id,
    ...opts,
  });
}

export function useFindingsQuery(id: string, opts?: QOpts<FindingDto[]>) {
  return useQuery<FindingDto[]>({
    queryKey: sessionKeys.findings(id),
    queryFn: async () => {
      const all = await getFindings(id);
      // Evidence-linked invariant: drop empty evidence_refs.
      return all.filter((f) => Array.isArray(f.evidence_refs) && f.evidence_refs.length > 0);
    },
    enabled: !!id,
    ...opts,
  });
}

export function useFindingEvidenceQuery(
  findingId: string,
  opts?: QOpts<FindingEvidenceResponse>,
) {
  return useQuery<FindingEvidenceResponse>({
    queryKey: sessionKeys.findingEvidence(findingId),
    queryFn: () => getFindingEvidence(findingId),
    enabled: !!findingId,
    ...opts,
  });
}

export function useVerificationRunsQuery(id: string, opts?: QOpts<VerificationRunDto[]>) {
  return useQuery<VerificationRunDto[]>({
    queryKey: sessionKeys.verificationRuns(id),
    queryFn: () => getVerificationRuns(id),
    enabled: !!id,
    ...opts,
  });
}

export function useSessionUsageQuery(id: string, opts?: QOpts<SessionUsageDto>) {
  return useQuery<SessionUsageDto>({
    queryKey: sessionKeys.usage(id),
    queryFn: () => getSessionUsage(id),
    enabled: !!id,
    ...opts,
  });
}

export function useDiffHunksQuery(id: string, opts?: QOpts<DiffHunkDto[]>) {
  return useQuery<DiffHunkDto[]>({
    queryKey: sessionKeys.diffHunks(id),
    queryFn: () => getDiffHunks(id),
    enabled: !!id,
    ...opts,
  });
}

export function useEventRawQuery(eventId: string | null) {
  return useQuery<RawEventResponse>({
    queryKey: ['eventRaw', eventId],
    queryFn: () => getEventRaw(eventId as string),
    enabled: !!eventId,
    staleTime: 60_000,
  });
}
