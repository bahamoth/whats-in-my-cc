import type {
  Envelope,
  SessionListItem,
  SessionDetail,
  GraphPayload,
  RawEventResponse,
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
export const getSession   = (id: string) => jsonGet<SessionDetail>(`/v1/sessions/${encodeURIComponent(id)}?limit=5000`);
export const getGraph     = (id: string) => jsonGet<GraphPayload>(`/v1/sessions/${encodeURIComponent(id)}/graph`);
export const getEventRaw  = (eventId: string) =>
  jsonGet<RawEventResponse>(`/v1/events/${encodeURIComponent(eventId)}/raw`);
