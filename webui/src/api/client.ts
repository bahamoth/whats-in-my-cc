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

// ---- PR-2: new Pull API helpers --------------------------------------
// All endpoints below are read-only. The backend wraps each payload in
// either `{ data: T }` (most) or `{ hunks: T[] }` (diff-hunks legacy)
// inside the standard `{ meta, data }` envelope. We unwrap once here so
// callers handle plain typed values.

export const getEpisodes = (id: string): Promise<EpisodeDto[]> =>
  jsonGet<{ data: EpisodeDto[] }>(`/v1/sessions/${encodeURIComponent(id)}/episodes`).then(
    (r) => r.data,
  );

export const getFindings = (id: string): Promise<FindingDto[]> =>
  jsonGet<{ data: FindingDto[] }>(`/v1/sessions/${encodeURIComponent(id)}/findings`).then(
    (r) => r.data,
  );

export const getFindingEvidence = (findingId: string): Promise<FindingEvidenceResponse> =>
  jsonGet<{ data: FindingEvidenceResponse }>(
    `/v1/findings/${encodeURIComponent(findingId)}/evidence`,
  ).then((r) => r.data);

export const getVerificationRuns = (id: string): Promise<VerificationRunDto[]> =>
  jsonGet<{ data: VerificationRunDto[] }>(
    `/v1/sessions/${encodeURIComponent(id)}/verification-runs`,
  ).then((r) => r.data);

export const getDiffHunks = (id: string): Promise<DiffHunkDto[]> =>
  jsonGet<{ hunks: DiffHunkDto[] }>(`/v1/sessions/${encodeURIComponent(id)}/diff-hunks`).then(
    (r) => r.hunks,
  );
