// webui/src/components/replay/stream/streamModel.ts
import type { ObservedEventDto } from '../../../api/types';

export type StreamCardKind = 'user' | 'assistant' | 'thinking' | 'tool';

export interface ToolResultView {
  isError: boolean;
  preview: string;
}

export interface ToolCardView {
  toolName: string | null;
  toolUseId: string | null;
  inputSummary: string;
  result: ToolResultView | null;
}

export interface StreamCard {
  id: string;
  kind: StreamCardKind;
  actor: string;
  timestamp: string;
  preview: string;
  tool: ToolCardView | null;
  /** Source event id, for graph-node correlation in the host. */
  eventId: string;
}

const CONVERSATION_KINDS = new Set(['user_message', 'assistant_message', 'thinking', 'tool_call']);

function asObj(v: unknown): Record<string, unknown> {
  return v && typeof v === 'object' ? (v as Record<string, unknown>) : {};
}

function toolInputSummary(toolName: string | null, input: unknown): string {
  const i = asObj(input);
  if (typeof i.command === 'string') return i.command;
  if (typeof i.file_path === 'string') return i.file_path;
  if (typeof i.pattern === 'string') return i.pattern;
  if (typeof i.skill === 'string') return i.skill;
  const keys = Object.keys(i);
  if (keys.length === 0) return toolName ?? '';
  try {
    return JSON.stringify(i);
  } catch {
    return keys.join(', ');
  }
}

function resultPreview(content: unknown): string {
  if (typeof content === 'string') return content;
  try {
    return JSON.stringify(content);
  } catch {
    return '';
  }
}

export function buildStreamCards(events: ObservedEventDto[]): StreamCard[] {
  // Index tool_result events by tool_use_id so we can merge them into calls.
  const resultsByToolUseId = new Map<string, ObservedEventDto>();
  for (const e of events) {
    if (e.kind === 'tool_result' && e.tool_use_id) {
      resultsByToolUseId.set(e.tool_use_id, e);
    }
  }

  const cards: StreamCard[] = [];
  for (const e of events) {
    if (!CONVERSATION_KINDS.has(e.kind)) continue;
    const p = asObj(e.payload);

    if (e.kind === 'user_message') {
      cards.push({ id: e.event_id, eventId: e.event_id, kind: 'user', actor: e.actor, timestamp: e.observed_at, preview: typeof p.content === 'string' ? p.content : '', tool: null });
    } else if (e.kind === 'assistant_message') {
      cards.push({ id: e.event_id, eventId: e.event_id, kind: 'assistant', actor: e.actor, timestamp: e.observed_at, preview: typeof p.text === 'string' ? p.text : '', tool: null });
    } else if (e.kind === 'thinking') {
      const t = typeof p.thinking === 'string' ? p.thinking : '';
      cards.push({ id: e.event_id, eventId: e.event_id, kind: 'thinking', actor: e.actor, timestamp: e.observed_at, preview: t.trim() === '' ? '(thinking redacted)' : t, tool: null });
    } else if (e.kind === 'tool_call') {
      const resultEv = e.tool_use_id ? resultsByToolUseId.get(e.tool_use_id) : undefined;
      let result: ToolResultView | null = null;
      if (resultEv) {
        const tr = asObj(asObj(resultEv.payload).tool_result);
        result = { isError: tr.is_error === true, preview: resultPreview(tr.content) };
      }
      cards.push({
        id: e.event_id, eventId: e.event_id, kind: 'tool', actor: e.actor, timestamp: e.observed_at,
        preview: '',
        tool: { toolName: e.tool_name, toolUseId: e.tool_use_id, inputSummary: toolInputSummary(e.tool_name, p.input), result },
      });
    }
  }
  return cards;
}
