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
  listSessions,
  getSessionUsage,
  getUsageBaseline,
  getVerificationRuns,
  getDiffHunks,
  getEventRaw,
  getCorrelatedEvents,
  getSessionMetrics,
  getSessionTurns,
  getPlugins,
  getSessionTasks,
  getVerificationSummary,
} from '../api/client';
import type {
  SessionDetail,
  SessionListItem,
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
  TaskDto,
  VerificationSummaryDto,
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
  verificationSummary: (id: string) => ['session', id, 'verification-summary'] as const,
};

/** 세션 목록 (store-global) — 세션 상세의 팀 배지·teammate 링크가 team 필드
 *  클라이언트 조인에 쓴다 (2026-07-03). */
export const sessionListKeys = {
  all: () => ['sessions'] as const,
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

/** Per-task summaries (status·duration·work-span aggregations), computed
 *  server-side by the `task_summary` aggregator. Drives the task list. */
export function useSessionTasksQuery(id: string, opts?: QOpts<TaskDto[]>) {
  return useQuery<TaskDto[]>({
    queryKey: sessionKeys.tasks(id),
    queryFn: () => getSessionTasks(id),
    enabled: !!id,
    ...opts,
  });
}

/** §3c — 세션 스코프 verification summary(변경 커버리지). 분석 패널 lazy. */
export function useSessionVerificationSummaryQuery(
  id: string,
  opts?: QOpts<VerificationSummaryDto>,
) {
  return useQuery<VerificationSummaryDto>({
    queryKey: sessionKeys.verificationSummary(id),
    queryFn: () => getVerificationSummary({ session_id: id }),
    enabled: !!id,
    ...opts,
  });
}

/** 세션 목록 — 팀 배지·teammate 링크의 조인 데이터원. 팀 관계는 세션 생성
 *  시점에 고정되므로 상세 화면에서는 느슨한 staleTime으로 충분하다. */
export function useSessionsListQuery(opts?: QOpts<SessionListItem[]>) {
  return useQuery<SessionListItem[]>({
    queryKey: sessionListKeys.all(),
    queryFn: listSessions,
    staleTime: 60_000,
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
