/**
 * Read-only React Query hooks for every session-scoped resource the UI needs.
 * Keys are arranged hierarchically so invalidation can target any level
 * (the whole session subtree, or a specific resource).
 *
 * Evidence-linked invariant: `useSignalsQuery` drops signals whose
 * `evidence_refs` array is empty. Per CLAUDE.md "Finding/RootCauseHypothesis/
 * QualitySummary는 evidence_refs 없이 만들지 않는다."
 */
import { useQuery, type UseQueryOptions } from '@tanstack/react-query';
import {
  getSession,
  getSignals,
  getSessionUsage,
  getUsageBaseline,
  getVerificationRuns,
  getDiffHunks,
  getEventRaw,
  getCorrelatedEvents,
  getSessionEvents,
  getSessionMetrics,
  getSessionTurns,
  getPlugins,
} from '../api/client';
import type {
  SessionDetail,
  SignalDto,
  SessionUsageDto,
  UsageBaselineDto,
  VerificationRunDto,
  DiffHunkDto,
  RawEventResponse,
  SessionEventsResponse,
  SessionMetricsDto,
  TurnRollupResponse,
  PluginDto,
} from '../api/types';

export const sessionKeys = {
  /** Root key for a session — invalidate this to wipe every nested cache
   *  (used on SSE `resync`). */
  session: (id: string) => ['session', id] as const,
  detail: (id: string) => ['session', id, 'detail'] as const,
  events: (id: string) => ['session', id, 'events'] as const,
  signals: (id: string) => ['session', id, 'signals'] as const,
  diffHunks: (id: string) => ['session', id, 'diff-hunks'] as const,
  verificationRuns: (id: string) => ['session', id, 'verification-runs'] as const,
  usage: (id: string) => ['session', id, 'usage'] as const,
  metrics: (id: string) => ['session', id, 'metrics'] as const,
  turns: (id: string) => ['session', id, 'turns'] as const,
  tasks: (id: string) => ['session', id, 'tasks'] as const,
};

/** insight-redesign #6 — store-wide usage baseline (not session-scoped). */
export const usageKeys = {
  baseline: () => ['usage', 'baseline'] as const,
};

/** Plugin registry is process-global on the server (not session-scoped). */
export const pluginKeys = {
  all: () => ['plugins'] as const,
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

export function useSignalsQuery(id: string, opts?: QOpts<SignalDto[]>) {
  return useQuery<SignalDto[]>({
    queryKey: sessionKeys.signals(id),
    queryFn: async () => {
      const all = await getSignals(id);
      // Evidence-linked invariant: drop empty evidence_refs.
      return all.filter((s) => Array.isArray(s.evidence_refs) && s.evidence_refs.length > 0);
    },
    enabled: !!id,
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

export function useSessionTurnsQuery(id: string, opts?: QOpts<TurnRollupResponse>) {
  return useQuery<TurnRollupResponse>({
    queryKey: sessionKeys.turns(id),
    queryFn: () => getSessionTurns(id),
    enabled: !!id,
    ...opts,
  });
}

/** Session-wide TaskCreate/TaskUpdate lifecycle, for the Task board. Fetches the
 *  whole session's tool_call+tool_result events (not the loaded replay window) so
 *  the board reflects ALL todos, then `buildTaskBoard` correlates them client-side.
 *  Bounded by the 5000 limit — a session with more than ~5000 tool events would
 *  miss its oldest tasks (acceptable for a local dogfooding tool). */
export function useSessionTasksQuery(id: string, opts?: QOpts<SessionEventsResponse>) {
  return useQuery<SessionEventsResponse>({
    queryKey: sessionKeys.tasks(id),
    queryFn: () => getSessionEvents(id, { kind: 'tool_call,tool_result', limit: 5000 }),
    enabled: !!id,
    ...opts,
  });
}

/** Plugin registry — long-lived (plugins rarely change mid-session). */
export function usePluginsQuery(opts?: QOpts<PluginDto[]>) {
  return useQuery<PluginDto[]>({
    queryKey: pluginKeys.all(),
    queryFn: () => getPlugins(),
    staleTime: 300_000,
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

export function useSessionMetricsQuery(id: string, opts?: QOpts<SessionMetricsDto>) {
  return useQuery<SessionMetricsDto>({
    queryKey: sessionKeys.metrics(id),
    queryFn: () => getSessionMetrics(id),
    enabled: !!id,
    ...opts,
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
