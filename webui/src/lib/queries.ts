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
  getFindings,
  getFindingEvidence,
  getSessionUsage,
  getUsageBaseline,
  getToolFailureSummary,
  getVerificationRuns,
  getDiffHunks,
  getEventRaw,
  getCorrelatedEvents,
} from '../api/client';
import type {
  SessionDetail,
  GraphPayload,
  FindingDto,
  SessionUsageDto,
  UsageBaselineDto,
  ToolFailureSummaryDto,
  VerificationRunDto,
  DiffHunkDto,
  FindingEvidenceResponse,
  RawEventResponse,
  SessionEventsResponse,
} from '../api/types';

export const sessionKeys = {
  /** Root key for a session — invalidate this to wipe every nested cache
   *  (used on SSE `resync`). */
  session: (id: string) => ['session', id] as const,
  detail: (id: string) => ['session', id, 'detail'] as const,
  graph: (id: string) => ['session', id, 'graph'] as const,
  events: (id: string) => ['session', id, 'events'] as const,
  findings: (id: string) => ['session', id, 'findings'] as const,
  toolFailures: (id: string) => ['session', id, 'tool-failures'] as const,
  diffHunks: (id: string) => ['session', id, 'diff-hunks'] as const,
  verificationRuns: (id: string) => ['session', id, 'verification-runs'] as const,
  usage: (id: string) => ['session', id, 'usage'] as const,
  findingEvidence: (findingId: string) => ['finding', findingId, 'evidence'] as const,
};

/** insight-redesign #6 — store-wide usage baseline (not session-scoped). */
export const usageKeys = {
  baseline: () => ['usage', 'baseline'] as const,
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

export function useToolFailureSummaryQuery(
  id: string,
  opts?: QOpts<ToolFailureSummaryDto>,
) {
  return useQuery<ToolFailureSummaryDto>({
    queryKey: sessionKeys.toolFailures(id),
    queryFn: () => getToolFailureSummary(id),
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

export function useUsageBaselineQuery(opts?: QOpts<UsageBaselineDto>) {
  return useQuery<UsageBaselineDto>({
    queryKey: usageKeys.baseline(),
    queryFn: () => getUsageBaseline(),
    staleTime: 60_000,
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

/** On-demand correlated telemetry for the selected entity's detail metrics,
 *  fetched by tool_use_id / request_id so metrics populate even when the
 *  telemetry falls outside the loaded message window. Disabled when neither
 *  key is present. */
export function useCorrelatedEventsQuery(
  sessionId: string,
  toolUseId: string | null,
  requestId: string | null,
) {
  return useQuery<SessionEventsResponse>({
    queryKey: ['correlated', sessionId, toolUseId, requestId],
    queryFn: () => getCorrelatedEvents(sessionId, { toolUseId, requestId }),
    enabled: !!toolUseId || !!requestId,
    staleTime: 60_000,
  });
}
