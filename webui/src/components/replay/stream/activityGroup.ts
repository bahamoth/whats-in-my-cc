import type { ActivityEvent } from './streamModel';
import { formatMcpToolName } from './nodeLabel';

export interface ActivityStackData { events: ActivityEvent[]; }
export interface StackSummary { count: number; topTools: string[]; errorCount: number; durationMs: number; }

/** Readable tool name for the collapsed activity-stack summary. Raw MCP names
 *  (`mcp__claude-in-chrome__computer`) → `claude-in-chrome · computer`, including
 *  when embedded in a hook name (`PreToolUse:mcp__…__computer` →
 *  `PreToolUse:claude-in-chrome · computer`). Non-MCP names pass through. */
function displayToolName(name: string): string {
  const colon = name.indexOf(':');
  if (colon > 0) {
    const rest = name.slice(colon + 1);
    return name.slice(0, colon + 1) + (formatMcpToolName(rest) ?? rest);
  }
  return formatMcpToolName(name) ?? name;
}

export function summarizeStack(stack: ActivityStackData): StackSummary {
  const counts = new Map<string, number>();
  let errorCount = 0;
  let min = Infinity, max = -Infinity;
  for (const { event, result } of stack.events) {
    const name = event.tool_name ?? event.kind;
    counts.set(name, (counts.get(name) ?? 0) + 1);
    if (result?.isError) errorCount++;
    const t = new Date(event.observed_at).getTime();
    if (!Number.isNaN(t)) { min = Math.min(min, t); max = Math.max(max, t); }
  }
  const topTools = [...counts.entries()].sort((a, b) => b[1] - a[1]).slice(0, 2)
    .map(([n, c]) => { const d = displayToolName(n); return c > 1 ? `${d} ×${c}` : d; });
  return { count: stack.events.length, topTools, errorCount, durationMs: max >= min ? max - min : 0 };
}
