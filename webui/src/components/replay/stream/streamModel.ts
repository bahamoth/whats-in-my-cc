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
  /** True when the source event is on a Task-tool sidechain (subagent). For a
   *  user_message this means the orchestrator's prompt TO a subagent — NOT human
   *  input — so it must not be labelled "You" nor right-aligned. */
  sidechain: boolean;
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

/** A contiguous run of sidechain (subagent) events — the prompt the orchestrator
 *  sent, the subagent's replies, and its tool activity — grouped so the whole
 *  exchange reads as one indented block separate from the main conversation. */
export interface SidechainGroup {
  type: 'sidechain-group';
  id: string;
  items: StreamItem[];
}

export type StreamItem = MessageItem | ActivityRun | SidechainGroup;

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

  // Sidechain items accumulate into a buffer that is flushed as one
  // SidechainGroup the moment the stream returns to the main thread (or ends).
  let scBuf: StreamItem[] | null = null;
  let scFirstId = '';
  const closeGroup = () => {
    if (scBuf && scBuf.length) {
      items.push({ type: 'sidechain-group', id: `sc-${scFirstId}`, items: scBuf });
    }
    scBuf = null;
  };
  const emit = (it: StreamItem, sidechain: boolean) => {
    if (sidechain) {
      if (!scBuf) { scBuf = []; scFirstId = it.id; }
      scBuf.push(it);
    } else {
      closeGroup();
      items.push(it);
    }
  };

  let run: ActivityEvent[] = [];
  let runSc = false;
  const flush = () => {
    if (run.length) {
      emit({ type: 'activity-run', id: `run-${run[0].event.event_id}`, events: run }, runSc);
      run = [];
    }
  };

  for (const e of events) {
    if (e.kind === 'tool_result') continue;
    const sc = !!e.is_sidechain;
    const c = classify(e);
    if (c.cat === 'message') {
      flush();
      emit(
        {
          type: 'message',
          id: e.event_id,
          eventId: e.event_id,
          role: c.role!,
          model: c.model ?? null,
          text: c.text!,
          timestamp: e.observed_at,
          sidechain: sc,
        },
        sc,
      );
    } else if (c.cat === 'activity') {
      // A change of sidechain status breaks the activity run so a run never
      // straddles the main↔subagent boundary.
      if (run.length && runSc !== sc) flush();
      runSc = sc;
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
  closeGroup();
  return items;
}

