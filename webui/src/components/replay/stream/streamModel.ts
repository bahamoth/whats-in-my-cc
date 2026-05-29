// webui/src/components/replay/stream/streamModel.ts
import type { ObservedEventDto } from '../../../api/types';

function asObj(v: unknown): Record<string, unknown> {
  return v && typeof v === 'object' ? (v as Record<string, unknown>) : {};
}

// ---------------------------------------------------------------------------
// buildStreamModel — Slice S2: stream classifier (message / activity / drop)
// ---------------------------------------------------------------------------

export type StreamRole = 'user' | 'assistant' | 'thinking';

export interface MessageItem {
  type: 'message';
  id: string;
  eventId: string;
  role: StreamRole;
  model: string | null;
  text: string;
  timestamp: string;
}

export interface ActivityEvent {
  event: ObservedEventDto;
  result: { isError: boolean } | null;
}

export interface ActivityRun {
  type: 'activity-run';
  id: string;
  events: ActivityEvent[];
}

export type StreamItem = MessageItem | ActivityRun;

const SCAFFOLD =
  /^\s*(<command-name>|<command-message>|<command-args>|<local-command-stdout>|<local-command-caveat>|Base directory for this skill:|\[Request interrupted)/;

function userText(p: Record<string, unknown>): string {
  return (typeof p.content === 'string'
    ? p.content
    : typeof p.text === 'string'
    ? p.text
    : ''
  ).trim();
}

function classify(
  e: ObservedEventDto,
): { cat: 'message' | 'activity' | 'drop'; role?: StreamRole; text?: string; model?: string | null } {
  const p = asObj(e.payload);
  if (e.kind === 'user_message') {
    const t = userText(p);
    if (t === '') return { cat: 'drop' };
    if (SCAFFOLD.test(t)) return { cat: 'activity' };
    return { cat: 'message', role: 'user', text: t, model: null };
  }
  if (e.kind === 'assistant_message') {
    const t = (typeof p.text === 'string' ? p.text : '').trim();
    return t
      ? { cat: 'message', role: 'assistant', text: t, model: (p.model as string) ?? null }
      : { cat: 'drop' };
  }
  if (e.kind === 'thinking') {
    const t = (typeof p.thinking === 'string' ? p.thinking : '').trim();
    return t ? { cat: 'message', role: 'thinking', text: t, model: null } : { cat: 'activity' };
  }
  if (e.kind === 'system_summary') return { cat: 'drop' };
  return { cat: 'activity' };
}

export function buildStreamModel(events: ObservedEventDto[]): StreamItem[] {
  const resultByUse = new Map<string, ObservedEventDto>();
  for (const e of events) {
    if (e.kind === 'tool_result' && e.tool_use_id) resultByUse.set(e.tool_use_id, e);
  }

  const items: StreamItem[] = [];
  let run: ActivityEvent[] = [];
  const flush = () => {
    if (run.length) {
      items.push({ type: 'activity-run', id: `run-${run[0].event.event_id}`, events: run });
      run = [];
    }
  };

  for (const e of events) {
    if (e.kind === 'tool_result') continue;
    const c = classify(e);
    if (c.cat === 'message') {
      flush();
      items.push({
        type: 'message',
        id: e.event_id,
        eventId: e.event_id,
        role: c.role!,
        model: c.model ?? null,
        text: c.text!,
        timestamp: e.observed_at,
      });
    } else if (c.cat === 'activity') {
      let result: { isError: boolean } | null = null;
      if (e.kind === 'tool_call' && e.tool_use_id) {
        const r = resultByUse.get(e.tool_use_id);
        if (r) result = { isError: asObj(asObj(r.payload).tool_result).is_error === true };
      }
      run.push({ event: e, result });
    }
    // cat === 'drop': skip silently
  }
  flush();
  return items;
}

