import type {
  Envelope,
  SessionListItem,
  SessionDetail,
  SessionEventsResponse,
  RawEventResponse,
  SignalDto,
  VerificationRunDto,
  DiffHunkDto,
  SessionUsageDto,
  UsageBaselineDto,
  SessionMetricsDto,
  TurnRollupResponse,
} from './types';

export class ApiError extends Error {
  constructor(public status: number, public detail: string) {
    super(detail);
  }
}

async function jsonGet<T>(path: string): Promise<T> {
  const resp = await fetch(path, { headers: { accept: 'application/json' } });
  if (!resp.ok) {
    let detail = resp.statusText;
    try {
      const body = await resp.json();
      detail = body.detail ?? detail;
    } catch { /* ignore */ }
    throw new ApiError(resp.status, detail);
  }
  const env = (await resp.json()) as Envelope<T>;
  return env.data;
}

export const listSessions = () => jsonGet<SessionListItem[]>('/v1/sessions');

/** Slice-9 — summary only. The 5000-event cap from slice-8 (DEV-S8-14) is
 *  gone; pages call {@link getSessionEvents} for the actual event window. */
export const getSession   = (id: string) => jsonGet<SessionDetail>(`/v1/sessions/${encodeURIComponent(id)}`);

export const getEventRaw  = (eventId: string) =>
  jsonGet<RawEventResponse>(`/v1/events/${encodeURIComponent(eventId)}/raw`);

/** Slice-9 — cursor-paged event window. `before`/`after` cursors have the
 *  shape `<observed_at_rfc3339>|<event_id>` and accept either ULIDs or the
 *  composite event_ids slice-6 emits for OTel metrics/logs.
 *
 *  `around` is the deep-link window: a bare event_id (no cursor — the client
 *  doesn't know the event's observed_at) for which the server returns the
 *  window containing that event (half the limit before, half after). 404 when
 *  the event is not in the session. Takes precedence over before/after. */
export function getSessionEvents(
  id: string,
  opts?: { before?: string; after?: string; around?: string; limit?: number; kind?: string },
): Promise<SessionEventsResponse> {
  const params = new URLSearchParams();
  if (opts?.before) params.set('before', opts.before);
  if (opts?.after) params.set('after', opts.after);
  if (opts?.around) params.set('around', opts.around);
  if (opts?.limit !== undefined) params.set('limit', String(opts.limit));
  if (opts?.kind) params.set('kind', opts.kind);
  const qs = params.toString();
  const path =
    `/v1/sessions/${encodeURIComponent(id)}/events` + (qs ? `?${qs}` : '');
  return jsonGet<SessionEventsResponse>(path);
}

/** On-demand correlated telemetry for the detail view: the events whose payload
 *  carries the given tool_use_id / request_id. Used when an entity's correlated
 *  telemetry falls outside the loaded message window. */
export function getCorrelatedEvents(
  id: string,
  opts: { toolUseId?: string | null; requestId?: string | null },
): Promise<SessionEventsResponse> {
  const params = new URLSearchParams();
  if (opts.toolUseId) params.set('tool_use_id', opts.toolUseId);
  if (opts.requestId) params.set('request_id', opts.requestId);
  return jsonGet<SessionEventsResponse>(
    `/v1/sessions/${encodeURIComponent(id)}/events?${params.toString()}`,
  );
}

// ---- Pull API helpers ------------------------------------------

export const getSignals = (id: string): Promise<SignalDto[]> =>
  jsonGet<SignalDto[]>(`/v1/sessions/${encodeURIComponent(id)}/signals`);

export const getVerificationRuns = (id: string): Promise<VerificationRunDto[]> =>
  // jsonGet already returns the envelope's `data`, which here IS the array.
  jsonGet<VerificationRunDto[]>(`/v1/sessions/${encodeURIComponent(id)}/verification-runs`);

export const getSessionUsage = (id: string): Promise<SessionUsageDto> =>
  jsonGet<SessionUsageDto>(`/v1/sessions/${encodeURIComponent(id)}/usage`);

/** insight-redesign #6 — cross-session usage baseline (no session id; this is
 *  a store-wide aggregate). The UI computes per-session deltas client-side. */
export const getUsageBaseline = (): Promise<UsageBaselineDto> =>
  jsonGet<UsageBaselineDto>('/v1/usage/baseline');

export const getDiffHunks = (id: string): Promise<DiffHunkDto[]> =>
  jsonGet<{ hunks: DiffHunkDto[] }>(`/v1/sessions/${encodeURIComponent(id)}/diff-hunks`).then(
    (r) => r.hunks,
  );

export const getSessionMetrics = (id: string): Promise<SessionMetricsDto> =>
  jsonGet<SessionMetricsDto>(`/v1/sessions/${encodeURIComponent(id)}/metrics`);

/** S8 — per-turn rollup (incl. per-turn token sums) for the KPI sparklines. */
export const getSessionTurns = (id: string): Promise<TurnRollupResponse> =>
  jsonGet<TurnRollupResponse>(`/v1/sessions/${encodeURIComponent(id)}/turns`);
