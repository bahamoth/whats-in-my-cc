import type {
  Envelope,
  SessionListItem,
  SessionDetail,
  SessionEventsResponse,
  GraphPayload,
  RawEventResponse,
  EpisodeDto,
  FindingDto,
  VerificationRunDto,
  DiffHunkDto,
  FindingEvidenceResponse,
  SessionUsageDto,
  ToolFailureSummaryDto,
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

export const getGraph     = (id: string) => jsonGet<GraphPayload>(`/v1/sessions/${encodeURIComponent(id)}/graph`);
export const getEventRaw  = (eventId: string) =>
  jsonGet<RawEventResponse>(`/v1/events/${encodeURIComponent(eventId)}/raw`);

/** Slice-9 — cursor-paged event window. `before`/`after` cursors have the
 *  shape `<observed_at_rfc3339>|<event_id>` and accept either ULIDs or the
 *  composite event_ids slice-6 emits for OTel metrics/logs. */
export function getSessionEvents(
  id: string,
  opts?: { before?: string; after?: string; limit?: number },
): Promise<SessionEventsResponse> {
  const params = new URLSearchParams();
  if (opts?.before) params.set('before', opts.before);
  if (opts?.after) params.set('after', opts.after);
  if (opts?.limit !== undefined) params.set('limit', String(opts.limit));
  const qs = params.toString();
  const path =
    `/v1/sessions/${encodeURIComponent(id)}/events` + (qs ? `?${qs}` : '');
  return jsonGet<SessionEventsResponse>(path);
}

// ---- PR-2/PR-6: read-only Pull API helpers ---------------------------
// Backend response shapes are NOT consistent today:
//   /episodes   -> { data: [...] }                  (no meta envelope)
//   /findings   -> { data: [...] }                  (no meta envelope)
//   /verification-runs -> { meta, data: [...] }      (data IS the array)
//   /diff-hunks        -> { meta, data: { hunks: [...] } }
//   /findings/:id/evidence -> { meta, data: { ... } }   (envelope envelope)
//
// `jsonGet` already returns `body.data` for us, so the *one* unwrap below
// targets the inner shape only. PR-6 fixed the original PR-2 helpers
// that assumed every endpoint was double-wrapped.

export const getEpisodes = (id: string): Promise<EpisodeDto[]> =>
  jsonGet<EpisodeDto[]>(`/v1/sessions/${encodeURIComponent(id)}/episodes`);

export const getFindings = (id: string): Promise<FindingDto[]> =>
  jsonGet<FindingDto[]>(`/v1/sessions/${encodeURIComponent(id)}/findings`);

export const getToolFailureSummary = (id: string): Promise<ToolFailureSummaryDto> =>
  jsonGet<ToolFailureSummaryDto>(`/v1/sessions/${encodeURIComponent(id)}/tool-failures`);

export const getFindingEvidence = (findingId: string): Promise<FindingEvidenceResponse> =>
  jsonGet<FindingEvidenceResponse>(`/v1/findings/${encodeURIComponent(findingId)}/evidence`);

export const getVerificationRuns = (id: string): Promise<VerificationRunDto[]> =>
  // jsonGet already returns the envelope's `data`, which here IS the array.
  jsonGet<VerificationRunDto[]>(`/v1/sessions/${encodeURIComponent(id)}/verification-runs`);

export const getSessionUsage = (id: string): Promise<SessionUsageDto> =>
  jsonGet<SessionUsageDto>(`/v1/sessions/${encodeURIComponent(id)}/usage`);

export const getDiffHunks = (id: string): Promise<DiffHunkDto[]> =>
  jsonGet<{ hunks: DiffHunkDto[] }>(`/v1/sessions/${encodeURIComponent(id)}/diff-hunks`).then(
    (r) => r.hunks,
  );
