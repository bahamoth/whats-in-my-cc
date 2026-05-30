// webui/src/components/replay/stream/streamModel.ts
import type { ObservedEventDto } from '../../../api/types';
import type { LlmRequestMetrics } from './llmRequestMetrics';

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

/** One redacted (content-less) thinking event. Claude Code transcripts store
 *  thinking blocks with an empty `thinking` text and an opaque `signature`
 *  only — the plaintext reasoning is recorded nowhere (not transcript, not
 *  OTel). So we can surface only that reasoning OCCURRED, when, and a rough
 *  size proxy from the signature length. We never attempt to decode it. */
export interface ThinkingEntry {
  eventId: string;
  timestamp: string;
  /** length of the encrypted `signature` — a rough proxy for reasoning size. */
  sigLen: number;
  /** request_id of the LLM response this thinking belongs to (join key). */
  requestId: string | null;
  /** per-response metrics (duration, tokens, …) joined via requestId. */
  metrics: LlmRequestMetrics | null;
}

/** One redacted thinking event, shown as a single compact, selectable inline
 *  marker in the conversation flow rather than buried in the activity stack.
 *  Selecting it surfaces the full per-response metrics in the side panel. */
export interface ThinkingMarker {
  type: 'thinking';
  id: string;
  events: ThinkingEntry[];
}

export type StreamItem = MessageItem | ActivityRun | SidechainGroup | ThinkingMarker;

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

/** log_record event_names that represent genuine state changes and should
 *  appear as beats in the message view. All other log_record names are
 *  telemetry / facet observations and are dropped from the stream. */
const STREAM_STATE_LOG = new Set([
  'compaction',
  'skill_activated',
  'permission_mode_changed',
  'mcp_server_connection',
]);

function classify(
  e: ObservedEventDto,
): {
  cat: 'message' | 'activity' | 'drop' | 'thinking';
  role?: StreamRole;
  text?: string;
  model?: string | null;
  sigLen?: number;
} {
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
    if (t) return { cat: 'message', role: 'thinking', text: t, model: null };
    // Redacted thinking: no plaintext anywhere, only an encrypted signature.
    // Surface it as a compact marker (not activity) so reasoning stays visible.
    const sig = typeof p.signature === 'string' ? p.signature : '';
    return { cat: 'thinking', sigLen: sig.length };
  }
  if (e.kind === 'system_summary') return { cat: 'drop' };
  if (e.kind === 'metric_sample' || e.kind === 'otel_span') return { cat: 'drop' };
  if (e.kind === 'log_record') {
    const name = (asObj(e.payload).event_name as string) ?? '';
    return STREAM_STATE_LOG.has(name) ? { cat: 'activity' } : { cat: 'drop' };
  }
  return { cat: 'activity' };
}

export function buildStreamModel(
  events: ObservedEventDto[],
  metricsByReq?: Map<string, LlmRequestMetrics>,
): StreamItem[] {
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
    } else if (c.cat === 'thinking') {
      // Close any open activity run first so order stays chronological. Each
      // redacted thinking is one LLM response → its own selectable marker
      // (no merging), so the side panel shows that one response's metrics.
      flush();
      const requestId = e.request_id ?? null;
      const metrics = requestId ? metricsByReq?.get(requestId) ?? null : null;
      emit(
        {
          type: 'thinking',
          id: `th-${e.event_id}`,
          events: [
            {
              eventId: e.event_id,
              timestamp: e.observed_at,
              sigLen: c.sigLen ?? 0,
              requestId,
              metrics,
            },
          ],
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

