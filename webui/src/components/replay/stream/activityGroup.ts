import type { ActivityEvent } from './streamModel';
import { displayToolName } from './nodeLabel';

export interface ActivityStackData { events: ActivityEvent[]; }
export interface StackSummary { count: number; topTools: string[]; errorCount: number; durationMs: number; }

export function summarizeStack(stack: ActivityStackData): StackSummary {
  const counts = new Map<string, number>();
  let errorCount = 0;
  let min = Infinity, max = -Infinity;
  for (const { event, result } of stack.events) {
    if (result?.isError) errorCount++;
    const t = new Date(event.observed_at).getTime();
    if (!Number.isNaN(t)) { min = Math.min(min, t); max = Math.max(max, t); }
    // Hooks (PreToolUse/PostToolUse) wrap a tool — fold them out of the tool
    // summary so it reads "mcp · …· computer (3 events)", not "computer ·
    // PreToolUse:computer · PostToolUse:computer". They still count toward the
    // total event count + errors above.
    if (event.kind === 'hook_event') continue;
    const name = event.tool_name ?? event.kind;
    counts.set(name, (counts.get(name) ?? 0) + 1);
  }
  // Fallback: a stack with ONLY hooks (the wrapped tool_call isn't in the loaded
  // window) — summarize by the hooks' underlying tool (the part after the
  // `Event:` prefix) so the header still says something.
  if (counts.size === 0) {
    for (const { event } of stack.events) {
      const raw = event.tool_name ?? event.kind;
      const colon = raw.indexOf(':');
      const name = event.kind === 'hook_event' && colon > 0 ? raw.slice(colon + 1) : raw;
      counts.set(name, (counts.get(name) ?? 0) + 1);
    }
  }
  const topTools = [...counts.entries()].sort((a, b) => b[1] - a[1]).slice(0, 2)
    .map(([n, c]) => { const d = displayToolName(n); return c > 1 ? `${d} ×${c}` : d; });
  return { count: stack.events.length, topTools, errorCount, durationMs: max >= min ? max - min : 0 };
}
